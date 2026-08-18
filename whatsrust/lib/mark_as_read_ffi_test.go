package main

import (
	"context"
	"os"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/types"
)

func TestMarkAsReadFFIOwnsMarkAsReadExport(t *testing.T) {
	seam, err := os.ReadFile("mark_as_read_ffi.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(seam), "//export C_MarkAsRead") {
		t.Fatal("mark-as-read FFI seam is missing C_MarkAsRead")
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "func C_MarkAsRead") {
		t.Fatal("mark-as-read FFI export remains in main.go")
	}
}

func TestMarkMessageAsReadPreservesRequest(t *testing.T) {
	wantMessageID := types.MessageID("message-42")
	wantChat := types.NewJID("chat-42", types.DefaultUserServer)
	wantSender := types.NewJID("sender-42", types.DefaultUserServer)
	before := time.Now()
	called := false

	markMessageAsRead(string(wantMessageID), wantChat, wantSender, func(ctx context.Context, messageIDs []types.MessageID, readAt time.Time, chat, sender types.JID, _ ...types.ReceiptType) error {
		called = true
		if ctx == nil || len(messageIDs) != 1 || messageIDs[0] != wantMessageID || chat != wantChat || sender != wantSender {
			t.Fatalf("mark-read request = (%v, %v, %v, %v), want preserved request", ctx, messageIDs, chat, sender)
		}
		if readAt.Before(before) || readAt.After(time.Now()) {
			t.Fatalf("read timestamp = %v, want current time", readAt)
		}
		return nil
	})

	if !called {
		t.Fatal("mark-read callback was not called")
	}
}
