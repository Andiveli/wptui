package main

import (
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

func TestNormalizeMessageInfoKeepsCanonicalIDs(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:   types.NewJID("chat", types.DefaultUserServer),
		Sender: types.NewJID("sender", types.DefaultUserServer),
	}}

	normalized := normalizeMessageInfo(info)
	if normalized.Chat != info.Chat || normalized.Sender != info.Sender {
		t.Fatalf("normalization changed canonical IDs: got chat=%s sender=%s", normalized.Chat, normalized.Sender)
	}
}

func TestNormalizeMessageInfoPromotesVerifiedSelfSenderAliases(t *testing.T) {
	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	previous := client
	client = &whatsmeow.Client{Store: &store.Device{
		ID:  &pn,
		LID: lid,
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	defer func() { client = previous }()

	for name, sender := range map[string]types.JID{
		"phone number": pn,
		"mapped LID":   lid,
		"device alias": types.NewADJID("123", 0, 7),
	} {
		t.Run(name, func(t *testing.T) {
			info := types.MessageInfo{MessageSource: types.MessageSource{Sender: sender}}
			if !normalizeMessageInfo(info).IsFromMe {
				t.Fatalf("sender %s was not promoted to self", sender)
			}
		})
	}
	other := types.MessageInfo{MessageSource: types.MessageSource{
		Sender: types.NewJID("999", types.DefaultUserServer),
	}}
	if normalizeMessageInfo(other).IsFromMe {
		t.Fatal("another user was incorrectly promoted to self")
	}
}

func TestDispatchMessageEventRecognizesReactionsAndActions(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:   types.NewJID("chat", types.DefaultUserServer),
		Sender: types.NewJID("sender", types.DefaultUserServer),
	}, ID: "action"}
	cases := []struct {
		name string
		msg  *waE2E.Message
	}{
		{
			name: "reaction",
			msg: &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
				Key:  &waCommon.MessageKey{ID: stringPointer("target")},
				Text: stringPointer("👍"),
			}},
		},
		{
			name: "revoke action",
			msg: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
				Key:  &waCommon.MessageKey{ID: stringPointer("target")},
				Type: waE2E.ProtocolMessage_REVOKE.Enum(),
			}},
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			if !dispatchMessageEvent(info, testCase.msg) {
				t.Fatal("message event was not dispatched")
			}
		})
	}
}

func TestDispatchMessageEventLeavesOrdinaryMessagesForCallback(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:   types.NewJID("chat", types.DefaultUserServer),
		Sender: types.NewJID("sender", types.DefaultUserServer),
	}}
	if dispatchMessageEvent(info, &waE2E.Message{Conversation: stringPointer("ordinary")}) {
		t.Fatal("ordinary message was dispatched as an action")
	}
}
