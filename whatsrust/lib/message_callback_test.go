package main

import (
	"testing"
	"time"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestMessageCallbackMetadataFromPreservesCallbackFields(t *testing.T) {
	info := types.MessageInfo{
		MessageSource: types.MessageSource{
			Chat:     types.NewJID("chat", "example.test"),
			Sender:   types.NewJID("sender", "example.test"),
			IsFromMe: true,
		},
		ID:        "message-id",
		Timestamp: time.Unix(123, 456),
	}
	forwarded := true
	message := &waE2E.Message{
		ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			ContextInfo: &waE2E.ContextInfo{IsForwarded: &forwarded, ForwardingScore: proto.Uint32(4)},
		},
	}

	metadata := messageCallbackMetadataFrom(info, message)
	if metadata.id != info.ID || metadata.chat != info.Chat || metadata.sender != info.Sender {
		t.Fatalf("callback identity metadata changed: %#v", metadata)
	}
	if metadata.timestamp != 123 || !metadata.isFromMe {
		t.Fatalf("callback scalar metadata changed: %#v", metadata)
	}
	if metadata.forwarding != (forwardingState{isForwarded: true, score: 4}) {
		t.Fatalf("callback forwarding metadata changed: %#v", metadata.forwarding)
	}
}

func TestBeginMessageCallbackSerializesCallbacks(t *testing.T) {
	info := types.MessageInfo{Timestamp: time.Unix(1, 0)}
	first := beginMessageCallback(info, &waE2E.Message{}, []byte("source"))
	acquired := make(chan struct{})
	done := make(chan *messageCallback)
	go func() {
		callback := beginMessageCallback(info, &waE2E.Message{}, nil)
		close(acquired)
		done <- callback
	}()

	select {
	case <-acquired:
		t.Fatal("second callback acquired the callback lock early")
	case <-time.After(10 * time.Millisecond):
	}
	first.close()

	select {
	case second := <-done:
		second.close()
	case <-time.After(time.Second):
		t.Fatal("second callback did not acquire the callback lock")
	}
}
