package main

import (
	"testing"
	"time"

	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func TestMessageActionKindName(t *testing.T) {
	tests := []struct {
		name string
		kind uint8
		want string
	}{
		{name: "edit", kind: messageActionEdit, want: "edit"},
		{name: "delete", kind: messageActionDelete, want: "delete"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := messageActionKindName(tt.kind); got != tt.want {
				t.Fatalf("messageActionKindName(%d) = %q, want %q", tt.kind, got, tt.want)
			}
		})
	}
}

func TestMessageActionEventFromMessage(t *testing.T) {
	baseInfo := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: types.NewJID("chat", types.DefaultUserServer), Sender: types.NewJID("sender", types.DefaultUserServer)},
		ID:            "action-id",
		Timestamp:     time.Unix(100, 0),
	}
	tests := []struct {
		name      string
		message   *waE2E.Message
		wantKind  uint8
		wantText  string
		wantTime  int64
		wantMatch bool
	}{
		{
			name: "edited wrapper preserves target replacement and protocol timestamp",
			message: &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
				Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
				Key:           &waCommon.MessageKey{ID: stringPointer("target")},
				EditedMessage: &waE2E.Message{Conversation: stringPointer("replacement")},
				TimestampMS:   int64Pointer(42_000),
			}}}},
			wantKind: messageActionEdit, wantText: "replacement", wantTime: 42, wantMatch: true,
		},
		{
			name: "revoke preserves target and envelope timestamp",
			message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
				Type: waE2E.ProtocolMessage_REVOKE.Enum(), Key: &waCommon.MessageKey{ID: stringPointer("target")},
			}},
			wantKind: messageActionDelete, wantTime: 100, wantMatch: true,
		},
		{name: "unsupported protocol is ignored", message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{}}, wantMatch: false},
		{name: "edit without replacement is ignored", message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(), Key: &waCommon.MessageKey{ID: stringPointer("target")}}}, wantMatch: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			action, ok := messageActionEventFromMessage(baseInfo, tt.message)
			if ok != tt.wantMatch {
				t.Fatalf("matched = %v, want %v", ok, tt.wantMatch)
			}
			if !ok {
				return
			}
			if action.actionID != "action-id" || action.targetMessageID != "target" || action.kind != tt.wantKind || action.replacement != tt.wantText || action.occurredAt != tt.wantTime {
				t.Fatalf("action = %#v", action)
			}
		})
	}
}
