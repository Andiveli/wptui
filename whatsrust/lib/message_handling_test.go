package main

import (
	"strings"
	"testing"
)

func TestHandleMessageOrchestrationOwnership(t *testing.T) {
	mainSource := string(mustRead(t, "main.go"))
	source := string(mustRead(t, "message_handling.go"))

	if strings.Contains(mainSource, "func HandleMessage(") {
		t.Fatal("HandleMessage orchestration must not remain in the CGO ABI root")
	}
	for _, fragment := range []string{
		"#include \"callback_log_registration.h\"",
		"func HandleMessage(",
		"msg, viewOnceUnavailable := unavailableViewOnceMessage(msg)",
		"info = normalizeMessageInfo(info)",
		"if dispatchMessageEvent(info, msg)",
		"rawSource := forwardingSourcePayload(info, msg, viewOnceUnavailable)",
		"callback := beginMessageCallback(info, msg, rawSource)",
		"defer callback.close()",
	} {
		if !strings.Contains(source, fragment) {
			t.Fatalf("message orchestration ownership is missing: %q", fragment)
		}
	}
	for _, fragment := range []string{
		"emitTextMessage(cinfo, msg.GetConversation(), isSync)",
		"emitTextMessage(cinfo, textWithMentionNames(ext_msg.GetText(), context_info), isSync)",
		"emitImageMessage(cinfo, info.ID, msg.GetImageMessage(), isSync)",
		"emitVideoMessage(cinfo, info.ID, msg.GetVideoMessage(), isSync)",
		"emitAudioMessage(cinfo, info.ID, msg.GetAudioMessage(), isSync)",
		"emitDocumentMessage(cinfo, info.ID, msg.GetDocumentMessage(), isSync)",
		"emitStickerMessage(cinfo, info.ID, msg.GetStickerMessage(), isSync)",
	} {
		if !strings.Contains(source, fragment) {
			t.Fatalf("message orchestration call is missing: %q", fragment)
		}
	}
}
