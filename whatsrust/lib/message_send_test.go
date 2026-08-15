package main

import (
	"os"
	"strings"
	"testing"
)

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
