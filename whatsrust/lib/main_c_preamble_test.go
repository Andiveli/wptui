package main

import (
	"os"
	"strings"
	"testing"
)

func TestMainDoesNotOwnMovedCDeclarations(t *testing.T) {
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}

	for _, declaration := range []string{
		"typedef struct {",
		"typedef void (*QrCallback)",
		"typedef void (*HistorySyncCallback)",
		"callEventCallback",
		"callQrCallback",
		"callMessageHandler",
		"callHistorySync",
	} {
		if strings.Contains(string(mainSource), declaration) {
			t.Fatalf("stale C declaration remains in main.go: %q", declaration)
		}
	}

	for _, owner := range []struct {
		file        string
		declaration string
	}{
		{"file_message_payload.go", "FileMessage"},
		{"receipt_events.go", "ReceiptEvent"},
		{"event_conversion.go", "ReactionEvent"},
		{"message_action_dispatch.go", "MessageActionEvent"},
		{"sync_events.go", "ChatEvent"},
		{"connection_events.go", "LogoutResultEvent"},
	} {
		source, err := os.ReadFile(owner.file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(source), owner.declaration) {
			t.Fatalf("%s must own %s", owner.file, owner.declaration)
		}
	}
}
