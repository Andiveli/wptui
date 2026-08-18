package main

import (
	"testing"

	"google.golang.org/protobuf/proto"

	"go.mau.fi/whatsmeow/proto/waE2E"
)

func TestMessageCensusKindsReportsSortedPresentFields(t *testing.T) {
	message := &waE2E.Message{
		Conversation: proto.String("text"),
		ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			Text: proto.String("extended"),
		},
	}

	if got := messageCensusKinds(message); got != "conversation,extendedtextmessage" {
		t.Fatalf("message census kinds = %q", got)
	}
}

func TestMessageCensusKindsReportsEmptyMessagesAsNone(t *testing.T) {
	if got := messageCensusKinds(nil); got != "none" {
		t.Fatalf("nil message census kinds = %q", got)
	}
	if got := messageCensusKinds(&waE2E.Message{}); got != "none" {
		t.Fatalf("empty message census kinds = %q", got)
	}
}

func TestMessageCensusWrappersReportsNestedPaths(t *testing.T) {
	message := &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
		EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{}},
	}}}

	if got := messageCensusWrappers(message, "raw"); got != "raw:ephemeral:edited" {
		t.Fatalf("wrapper census path = %q", got)
	}
}
