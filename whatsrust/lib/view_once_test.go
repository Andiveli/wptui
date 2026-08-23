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
			placeholder, unavailable := unavailableViewOnceMessage(wrapped)
			if !unavailable {
				t.Fatal("view-once wrapper must be unavailable")
			}
			if got := placeholder.GetConversation(); got != viewOnceUnavailablePlaceholder {
				t.Fatalf("placeholder = %q, want %q", got, viewOnceUnavailablePlaceholder)
			}
			if placeholder.GetImageMessage() != nil || placeholder.GetVideoMessage() != nil {
				t.Fatal("view-once media must not reach the public message model")
			}
		})
	}
}

func TestOrdinaryAndEphemeralMessagesRemainAvailable(t *testing.T) {
	ordinary := &waE2E.Message{Conversation: stringPointer("ordinary")}
	if got, unavailable := unavailableViewOnceMessage(ordinary); unavailable || got != ordinary {
		t.Fatal("ordinary message must remain unchanged")
	}
	ephemeral := &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: ordinary}}
	if got, unavailable := unavailableViewOnceMessage(ephemeral); unavailable || got != ephemeral {
		t.Fatal("ephemeral message must remain unchanged")
	}
}
