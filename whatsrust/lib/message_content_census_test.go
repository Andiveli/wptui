package main

import (
	"testing"

	waE2E "go.mau.fi/whatsmeow/proto/waE2E"
)

func TestMessageContentTypesClassifiesMessageShapes(t *testing.T) {
	text := "text"
	tests := []struct {
		name string
		msg  *waE2E.Message
		want string
	}{
		{name: "nil", want: "nil"},
		{name: "empty", msg: &waE2E.Message{}, want: "empty"},
		{
			name: "ordinary text shapes",
			msg:  &waE2E.Message{Conversation: &text, ExtendedTextMessage: &waE2E.ExtendedTextMessage{}},
			want: "conversation,extended_text",
		},
		{
			name: "protocol edit",
			msg:  &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{EditedMessage: &waE2E.Message{}}},
			want: "protocol+edited",
		},
		{
			name: "nested edit shapes",
			msg: &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
				Conversation: &text, ExtendedTextMessage: &waE2E.ExtendedTextMessage{},
			}}},
			want: "edited+inner_conv_ext",
		},
		{
			name: "reaction",
			msg:  &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{}},
			want: "reaction",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := msgContentTypes(tt.msg); got != tt.want {
				t.Fatalf("msgContentTypes() = %q, want %q", got, tt.want)
			}
		})
	}
}
