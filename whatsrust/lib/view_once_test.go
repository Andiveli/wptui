package main

import (
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
)

func TestViewOnceWrappersBecomeUnavailablePlaceholders(t *testing.T) {
	for _, testCase := range []struct {
		name string
		wrap func(*waE2E.Message) *waE2E.Message
	}{
		{"v1", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"v2", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"v2 extension", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2Extension: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"ephemeral v2", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{ViewOnceMessageV2: &waE2E.FutureProofMessage{Message: message}}}}
		}},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			wrapped := testCase.wrap(&waE2E.Message{ImageMessage: &waE2E.ImageMessage{Caption: stringPointer("private media")}})
			if !isUnavailableViewOnceMessage(wrapped) {
				t.Fatal("view-once wrapper must be unavailable")
			}
		})
	}
}

func TestOrdinaryAndEphemeralMessagesRemainAvailable(t *testing.T) {
	ordinary := &waE2E.Message{Conversation: stringPointer("ordinary")}
	if isUnavailableViewOnceMessage(ordinary) {
		t.Fatal("ordinary message must remain available")
	}
	ephemeral := &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: ordinary}}
	if isUnavailableViewOnceMessage(ephemeral) {
		t.Fatal("ephemeral message must remain available")
	}
}
