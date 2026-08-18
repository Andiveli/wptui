package main

import (
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestForwardingSourcePayloadCachesAndSerializesMessage(t *testing.T) {
	resetForwardedSourcesForTest()
	t.Cleanup(resetForwardedSourcesForTest)
	chat := types.NewJID("chat", types.DefaultUserServer)
	info := types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: chat}, ID: "message"}
	message := &waE2E.Message{Conversation: stringPointer("payload")}

	raw := forwardingSourcePayload(info, message, false)
	decoded, err := proto.Marshal(message)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != string(decoded) {
		t.Fatalf("serialized source = %x, want %x", raw, decoded)
	}
	key := forwardSourceKey(chat, chat, info.ID)
	if _, ok := forwardedSources.entries[key]; !ok {
		t.Fatal("forwarding source was not cached")
	}
}

func TestForwardingSourcePayloadSkipsUnavailableViewOnceMessage(t *testing.T) {
	resetForwardedSourcesForTest()
	t.Cleanup(resetForwardedSourcesForTest)
	chat := types.NewJID("chat", types.DefaultUserServer)
	info := types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: chat}, ID: "view-once"}
	message := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{}}

	if raw := forwardingSourcePayload(info, message, true); raw != nil {
		t.Fatalf("unavailable source bytes = %x, want nil", raw)
	}
	if len(forwardedSources.entries) != 0 {
		t.Fatal("unavailable view-once message must not be cached")
	}
}
