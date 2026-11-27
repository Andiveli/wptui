package main

import (
	"slices"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestHistoryMessageDispatchContinuesAfterBodylessGroupStub(t *testing.T) {
	previousObserve := observeTextCallback
	t.Cleanup(func() { observeTextCallback = previousObserve })
	var received []string
	observeTextCallback = func(output textCallbackOutput) {
		received = append(received, output.text)
	}

	client := &whatsmeow.Client{}
	chat := types.NewJID("123", types.GroupServer)
	participant := types.NewJID("456", types.DefaultUserServer).String()
	entries := []*waWeb.WebMessageInfo{
		{Message: &waE2E.Message{Conversation: proto.String("before")}},
		{MessageStubType: waWeb.WebMessageInfo_GROUP_PARTICIPANT_ADD.Enum()},
		{Message: &waE2E.Message{Conversation: proto.String("after")}},
	}
	for i, entry := range entries {
		entry.Key = &waCommon.MessageKey{Participant: &participant}
		parsed, err := client.ParseWebMessage(chat, entry)
		if err != nil {
			t.Fatalf("parse history entry %d: %v", i, err)
		}
		dispatchIncomingMessage(parsed, func(messageActionEvent) {
			t.Fatalf("entry %d unexpectedly dispatched a message action", i)
		}, func(info types.MessageInfo, message *waE2E.Message, _ bool) {
			HandleMessage(info, message, true)
		})
	}
	if want := []string{"before", "after"}; !slices.Equal(received, want) {
		t.Fatalf("emitted texts = %q, want %q", received, want)
	}
}

func TestHandleMessageOrchestrationOwnership(t *testing.T) {
	mainSource := string(mustRead(t, "main.go"))
	source := string(mustRead(t, "message_handling.go"))

	if strings.Contains(mainSource, "func HandleMessage(") {
		t.Fatal("HandleMessage orchestration must not remain in the CGO ABI root")
	}
	for _, fragment := range []string{
		"#include \"callback_log_registration.h\"",
		"func HandleMessage(",
		"func HandleViewOnceUnavailableMessage(info types.MessageInfo, isSync bool)",
		"info = normalizeMessageInfo(info)",
		"if dispatchMessageEvent(info, msg)",
		"rawSource := forwardingSourcePayload(info, msg, false)",
		"callback := beginMessageCallback(info, msg, rawSource)",
		"defer callback.close()",
	} {
		if !strings.Contains(source, fragment) {
			t.Fatalf("message orchestration ownership is missing: %q", fragment)
		}
	}
	for _, fragment := range []string{
		"emitTextMessage(cinfo, msg.GetConversation(), isSync)",
		"emitTextMessage(cinfo, text, isSync)",
		"emitImageMessage(callback, info.ID, msg.GetImageMessage(), isSync)",
		"emitVideoMessage(callback, info.ID, msg.GetVideoMessage(), isSync)",
		"emitAudioMessage(callback, info.ID, msg.GetAudioMessage(), isSync)",
		"emitDocumentMessage(callback, info.ID, msg.GetDocumentMessage(), isSync)",
		"emitStickerMessage(callback, info.ID, msg.GetStickerMessage(), isSync)",
	} {
		if !strings.Contains(source, fragment) {
			t.Fatalf("message orchestration call is missing: %q", fragment)
		}
	}
}
