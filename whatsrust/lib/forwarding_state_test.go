package main

import (
	"testing"

	waE2E "go.mau.fi/whatsmeow/proto/waE2E"
	"google.golang.org/protobuf/proto"
)

func TestForwardingStateExtractsEverySupportedMessageContext(t *testing.T) {
	forwarded, score := proto.Bool(true), uint32(5)
	context := &waE2E.ContextInfo{IsForwarded: forwarded, ForwardingScore: &score}
	for _, message := range []*waE2E.Message{
		{ExtendedTextMessage: &waE2E.ExtendedTextMessage{ContextInfo: context}},
		{ImageMessage: &waE2E.ImageMessage{ContextInfo: context}}, {VideoMessage: &waE2E.VideoMessage{ContextInfo: context}},
		{AudioMessage: &waE2E.AudioMessage{ContextInfo: context}}, {DocumentMessage: &waE2E.DocumentMessage{ContextInfo: context}},
		{StickerMessage: &waE2E.StickerMessage{ContextInfo: context}},
	} {
		state := forwardingStateFromMessage(message)
		if !state.isForwarded || state.score != 5 {
			t.Fatalf("forwarding state = %#v", state)
		}
	}
	if state := forwardingStateFromMessage(&waE2E.Message{Conversation: proto.String("text")}); state != (forwardingState{}) {
		t.Fatalf("conversation forwarding state = %#v", state)
	}
}
