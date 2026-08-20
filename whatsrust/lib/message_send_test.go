package main

import (
	"context"
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
)

func TestOutboundWirePayloadCarriesTextMentionMetadata(t *testing.T) {
	message := contentToWaE2EMessage(
		MessageTypeText,
		"hello @123",
		[]string{"123@lid"},
		0,
		"",
		nil,
		&waE2E.ContextInfo{},
		nil,
	)
	contextInfo := message.GetExtendedTextMessage().GetContextInfo()
	if got := contextInfo.GetMentionedJID(); len(got) != 1 || got[0] != "123@lid" {
		t.Fatalf("text protobuf MentionedJID = %v, want [123@lid]", got)
	}
}

func TestOutboundWirePayloadCarriesCaptionMentionMetadata(t *testing.T) {
	file, err := os.CreateTemp(t.TempDir(), "mention-image.png")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString("image"); err != nil {
		t.Fatal(err)
	}
	file.Close()

	caption := "hello @123"
	message := contentToWaE2EMessage(
		MessageTypeFile,
		"",
		[]string{"123@s.whatsapp.net"},
		FileTypeImage,
		file.Name(),
		&caption,
		&waE2E.ContextInfo{},
		func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
			return whatsmeow.UploadResponse{URL: "https://example.test/image", DirectPath: "/image"}, nil
		},
	)
	contextInfo := message.GetImageMessage().GetContextInfo()
	if got := contextInfo.GetMentionedJID(); len(got) != 1 || got[0] != "123@s.whatsapp.net" {
		t.Fatalf("caption protobuf MentionedJID = %v, want [123@s.whatsapp.net]", got)
	}
}

func TestSetMentionedJIDsPreservesFileCaptionMetadata(t *testing.T) {
	contextInfo := &waE2E.ContextInfo{}
	setMentionedJIDsFromStrings(contextInfo, []string{"111@s.whatsapp.net"})

	if got := contextInfo.GetMentionedJID(); len(got) != 1 || got[0] != "111@s.whatsapp.net" {
		t.Fatalf("MentionedJID = %v, want [111@s.whatsapp.net]", got)
	}
}

func TestSetMentionedJIDsAcceptsNullContext(t *testing.T) {
	setMentionedJIDsFromStrings(nil, []string{"111@s.whatsapp.net"})
}

func TestQuotedMessageFromContentPreservesTextAndFileKindsWithoutUpload(t *testing.T) {
	quotedText := quotedTextMessage("quoted text")
	if quotedText.GetConversation() != "quoted text" {
		t.Fatalf("quoted text = %#v", quotedText)
	}

	quotedImage := quotedFileMessage(FileTypeImage, "caption")
	if quotedImage.GetImageMessage() == nil || quotedImage.GetImageMessage().GetCaption() != "caption" {
		t.Fatalf("quoted image = %#v", quotedImage)
	}
	if quotedFileMessage(99, "") != nil {
		t.Fatal("unknown quoted file kind must be omitted")
	}
}

func TestQuotedMessageBuildersPreserveTextAndFileKinds(t *testing.T) {
	if quotedTextMessage("quoted text").GetConversation() != "quoted text" {
		t.Fatal("quoted text was not preserved")
	}
	if quotedFileMessage(FileTypeImage, "caption").GetImageMessage().GetCaption() != "caption" {
		t.Fatal("image quote caption was not preserved")
	}
	if quotedFileMessage(FileTypeAudio, "ignored").GetAudioMessage() == nil {
		t.Fatal("audio quote was not built")
	}
	if quotedFileMessage(99, "caption") != nil {
		t.Fatal("unknown quoted file kind should be omitted")
	}
}

func TestQuotedContextInfoPreservesOriginalAttribution(t *testing.T) {
	context := quotedContextInfo("message-id", "sender@s.whatsapp.net", "chat@s.whatsapp.net")
	if context.GetStanzaID() != "message-id" || context.GetParticipant() != "sender@s.whatsapp.net" || context.GetRemoteJID() != "chat@s.whatsapp.net" {
		t.Fatalf("quote context = %+v, want original attribution", context)
	}
}

func TestMessageSendKeepsExportedBridgeContractInDedicatedOrchestration(t *testing.T) {
	source, err := os.ReadFile("message_send.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, fragment := range []string{
		"//export C_SendMessage",
		"func C_SendMessage(cjid C.JID",
		"HandleMessage(messageInfo, message, false)",
	} {
		if !strings.Contains(string(source), fragment) {
			t.Fatalf("message send orchestration missing %q", fragment)
		}
	}
}
