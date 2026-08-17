package main

import (
	"testing"
	"time"

	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/types"
)

func TestParseWebMessageInfoPreservesMetadataAndSenderPrecedence(t *testing.T) {
	self := types.NewJID("self", types.DefaultUserServer)
	chat := types.NewJID("group", types.GroupServer)
	cases := []struct {
		name        string
		fromMe      bool
		participant string
		keySender   string
		wantSender  types.JID
	}{
		{name: "from me uses self", fromMe: true, participant: "other@s.whatsapp.net", keySender: "key@s.whatsapp.net", wantSender: self},
		{name: "envelope participant wins", participant: "envelope@s.whatsapp.net", keySender: "key@s.whatsapp.net", wantSender: types.NewJID("envelope", types.DefaultUserServer)},
		{name: "key participant is fallback", keySender: "key@s.whatsapp.net", wantSender: types.NewJID("key", types.DefaultUserServer)},
		{name: "chat is final fallback", wantSender: chat},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			message := &waWeb.WebMessageInfo{
				Key:              &waCommon.MessageKey{FromMe: &testCase.fromMe, Participant: stringPointer(testCase.keySender), ID: stringPointer("message-id")},
				Participant:      stringPointer(testCase.participant),
				PushName:         stringPointer("Push Name"),
				MessageTimestamp: uint64Pointer(42),
			}
			got := ParseWebMessageInfo(self, chat, message)
			if got == nil || got.Sender != testCase.wantSender || got.Chat != chat || got.ID != "message-id" || got.PushName != "Push Name" || !got.IsGroup || !got.Timestamp.Equal(time.Unix(42, 0)) {
				t.Fatalf("parsed info = %#v, want sender %s and preserved metadata", got, testCase.wantSender)
			}
		})
	}
}

func uint64Pointer(value uint64) *uint64 { return &value }

func TestParseWebMessageInfoRejectsAnUnusableSender(t *testing.T) {
	message := &waWeb.WebMessageInfo{Key: &waCommon.MessageKey{ID: stringPointer("message-id")}}
	if got := ParseWebMessageInfo(types.JID{}, types.JID{}, message); got != nil {
		t.Fatalf("parsed info = %#v, want nil for empty sender", got)
	}
}
