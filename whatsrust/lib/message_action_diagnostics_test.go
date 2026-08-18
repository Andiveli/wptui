package main

import (
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestMessageActionStructuralDiagnosticsAreSignalScopedAndRedacted(t *testing.T) {
	chat := types.NewJID("chat-secret", types.DefaultUserServer)
	sender := types.NewJID("sender-secret", types.DefaultUserServer)
	info := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, Sender: sender},
		ID:            "action-secret",
	}
	ordinary := &events.Message{Info: info, Message: &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")}}
	if line := messageActionStructuralLine(ordinary, "ordinary", ""); line != "" {
		t.Fatalf("ordinary message emitted diagnostic: %s", line)
	}

	wrappers := []struct {
		name string
		wrap func(*waE2E.Message) *waE2E.Message
	}{
		{"edited", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"device sent", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{DeviceSentMessage: &waE2E.DeviceSentMessage{Message: message}}
		}},
		{"bot invoke", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{BotInvokeMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"ephemeral", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"view once", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"view once v2", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"view once v2 extension", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2Extension: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"lottie sticker", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{LottieStickerMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"document caption", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{DocumentWithCaptionMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
	}
	for _, testCase := range wrappers {
		t.Run(testCase.name, func(t *testing.T) {
			event := &events.Message{Info: info, RawMessage: testCase.wrap(wrappedEditFixture("target-secret", "DO-NOT-LOG-BODY")), IsEdit: true}
			line := messageActionStructuralLine(event, "raw_classified", "")
			if strings.Count(line, "classifier=structural") != 1 {
				t.Fatalf("diagnostic count = %d, want 1: %s", strings.Count(line, "classifier=structural"), line)
			}
			if !strings.Contains(line, "replacement_variant=conversation") || !strings.Contains(line, "target_id=<id:") {
				t.Fatalf("diagnostic omitted structural fields: %s", line)
			}
			for _, secret := range []string{"DO-NOT-LOG-BODY", "target-secret", "action-secret", chat.String(), sender.String()} {
				if strings.Contains(line, secret) {
					t.Fatalf("diagnostic leaked %q: %s", secret, line)
				}
			}
			if _, ok := messageActionEventFromIncomingMessage(event); !ok {
				t.Fatal("wrapped edit was not classified")
			}
		})
	}

	missingTarget := &events.Message{Info: info, RawMessage: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
		Key:           &waCommon.MessageKey{},
		Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
		EditedMessage: &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")},
	}}, IsEdit: true}
	line := messageActionStructuralLine(missingTarget, "raw_miss", "missing_target_id")
	if !strings.Contains(line, "target_id=<missing>") || !strings.Contains(line, "protocol_key=true protocol_key_id=false") {
		t.Fatalf("missing target was not explicit: %s", line)
	}
}
