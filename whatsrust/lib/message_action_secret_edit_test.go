package main

import (
	"context"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestIncomingSecretEncryptedMessageEditDispatch(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	editor := types.NewJID("0987654321", types.DefaultUserServer)
	secretEdit := func(target string, kind waE2E.SecretEncryptedMessage_SecretEncType) *waE2E.Message {
		return &waE2E.Message{SecretEncryptedMessage: &waE2E.SecretEncryptedMessage{
			TargetMessageKey: &waCommon.MessageKey{ID: stringPointer(target), Participant: stringPointer("target-author@s.whatsapp.net")},
			SecretEncType:    kind.Enum(),
			EncPayload:       []byte("ciphertext"),
			EncIV:            []byte("initial-vector"),
		}}
	}
	baseEvent := func(message *waE2E.Message) *events.Message {
		return &events.Message{Info: types.MessageInfo{
			MessageSource: types.MessageSource{Chat: chat, Sender: editor, IsGroup: true},
			ID:            "edit-action",
			Timestamp:     time.Unix(42, 0),
		}, RawMessage: message, Message: message}
	}

	cases := []struct {
		name             string
		event            *events.Message
		decrypt          decryptSecretEncryptedMessageFunc
		wantAction       bool
		wantReplacement  string
		wantOrdinary     int
		wantDecryptCalls int
	}{
		{
			name:  "conversation replacement dispatches one durable edit",
			event: baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			decrypt: func(_ context.Context, event *events.Message) (*waE2E.Message, error) {
				if event.Message.GetSecretEncryptedMessage() == nil {
					t.Fatal("decrypt did not receive the located secret envelope")
				}
				return &waE2E.Message{Conversation: stringPointer("replacement")}, nil
			},
			wantAction: true, wantReplacement: "replacement", wantDecryptCalls: 1,
		},
		{
			name:  "extended text replacement dispatches one durable edit",
			event: baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			decrypt: func(context.Context, *events.Message) (*waE2E.Message, error) {
				return &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("replacement")}}, nil
			},
			wantAction: true, wantReplacement: "replacement", wantDecryptCalls: 1,
		},
		{
			name: "device sent wrapper locates the secret edit",
			event: baseEvent(&waE2E.Message{DeviceSentMessage: &waE2E.DeviceSentMessage{
				Message: secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT),
			}}),
			decrypt: func(context.Context, *events.Message) (*waE2E.Message, error) {
				return &waE2E.Message{Conversation: stringPointer("replacement")}, nil
			},
			wantAction: true, wantReplacement: "replacement", wantDecryptCalls: 1,
		},
		{
			name:             "other secret types remain ordinary",
			event:            baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_EVENT_EDIT)),
			wantOrdinary:     1,
			wantDecryptCalls: 0,
		},
		{
			name:             "missing target remains ordinary without decryption",
			event:            baseEvent(secretEdit("", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			wantOrdinary:     1,
			wantDecryptCalls: 0,
		},
		{
			name:  "missing original secret preserves the envelope without action",
			event: baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			decrypt: func(context.Context, *events.Message) (*waE2E.Message, error) {
				return nil, whatsmeow.ErrOriginalMessageSecretNotFound
			},
			wantOrdinary: 1, wantDecryptCalls: 1,
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			decryptCalls := 0
			decrypt := testCase.decrypt
			if decrypt != nil {
				originalDecrypt := decrypt
				decrypt = func(ctx context.Context, event *events.Message) (*waE2E.Message, error) {
					decryptCalls++
					return originalDecrypt(ctx, event)
				}
			}
			var actions []messageActionEvent
			ordinaryMessages := 0
			originalMessage := testCase.event.Message
			dispatchIncomingMessageWithDecrypt(testCase.event, func(action messageActionEvent) {
				actions = append(actions, action)
			}, func(types.MessageInfo, *waE2E.Message, bool) {
				ordinaryMessages++
			}, decrypt)

			if len(actions) != map[bool]int{true: 1, false: 0}[testCase.wantAction] || ordinaryMessages != testCase.wantOrdinary || decryptCalls != testCase.wantDecryptCalls {
				t.Fatalf("actions=%d ordinary=%d decrypt_calls=%d", len(actions), ordinaryMessages, decryptCalls)
			}
			if !testCase.wantAction {
				if testCase.event.Message != originalMessage {
					t.Fatal("failed secret edit mutated the incoming envelope")
				}
				return
			}
			action := actions[0]
			if action.actionID != "edit-action" || action.targetMessageID != "target-message" || action.replacement != testCase.wantReplacement || action.chat != chat.String() || action.sender != editor.String() || action.occurredAt != 42 || action.kind != messageActionEdit {
				t.Fatalf("unexpected action: %#v", action)
			}
		})
	}
}

func TestSecretEditDiagnosticsAreRedacted(t *testing.T) {
	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "1")
	resetEventCensusForTest()
	event := &events.Message{Info: types.MessageInfo{ID: "action-secret"}, Message: &waE2E.Message{SecretEncryptedMessage: &waE2E.SecretEncryptedMessage{
		TargetMessageKey: &waCommon.MessageKey{ID: stringPointer("target-secret")},
		SecretEncType:    waE2E.SecretEncryptedMessage_MESSAGE_EDIT.Enum(),
		EncPayload:       []byte("DO-NOT-LOG-CIPHERTEXT"),
		EncIV:            []byte("DO-NOT-LOG-IV"),
	}}}

	if _, ok := messageActionEventFromSecretEncryptedMessage(event, func(context.Context, *events.Message) (*waE2E.Message, error) {
		return nil, whatsmeow.ErrOriginalMessageSecretNotFound
	}); ok {
		t.Fatal("failed decryption produced an action")
	}
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	if len(eventCensus.entries) != 1 {
		t.Fatalf("diagnostic entries = %d, want 1", len(eventCensus.entries))
	}
	line := eventCensus.entries[0]
	for _, secret := range []string{"action-secret", "target-secret", "DO-NOT-LOG-CIPHERTEXT", "DO-NOT-LOG-IV"} {
		if strings.Contains(line, secret) {
			t.Fatalf("diagnostic leaked %q: %s", secret, line)
		}
	}
	for _, field := range []string{"secret_edit_result=failed", "error_class=missing_original_secret", "secret_enc_type=message_edit", "secret_enc_type_number=2", "secret_payload_length=21", "secret_iv_length=13"} {
		if !strings.Contains(line, field) {
			t.Fatalf("diagnostic omitted %q: %s", field, line)
		}
	}
}
