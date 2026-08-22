package main

import (
	"context"
	"testing"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

func TestChatReadSyncBuildsOnePatchWithLatestIdentity(t *testing.T) {
	tests := []struct {
		name        string
		chat        string
		fromMe      bool
		participant string
		wantPart    string
	}{
		{name: "dm", chat: "alice@s.whatsapp.net", wantPart: ""},
		{name: "group", chat: "group@g.us", fromMe: true, participant: "alice@s.whatsapp.net", wantPart: "alice@s.whatsapp.net"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var calls int
			var got appstate.PatchInfo
			err := sendChatReadSync(context.Background(), tt.chat, "latest", 42, tt.fromMe, tt.participant,
				func(_ context.Context, patch appstate.PatchInfo) error {
					calls++
					got = patch
					return nil
				})
			if err != nil {
				t.Fatal(err)
			}
			if calls != 1 {
				t.Fatalf("send calls = %d, want 1", calls)
			}
			if len(got.Mutations) != 1 || got.Mutations[0].Index[1] != tt.chat {
				t.Fatalf("patch target = %#v, want %s", got.Mutations, tt.chat)
			}
			action := got.Mutations[0].Value.GetMarkChatAsReadAction()
			if action == nil || !action.GetRead() || action.GetMessageRange().GetLastMessageTimestamp() != 42 {
				t.Fatalf("action = %#v, want read at 42", action)
			}
			message := action.GetMessageRange().GetMessages()[0]
			if message.GetKey().GetRemoteJID() != tt.chat || message.GetKey().GetID() != "latest" || message.GetKey().GetFromMe() != tt.fromMe {
				t.Fatalf("message key = %#v, want chat/id/from-me truth", message.GetKey())
			}
			if message.GetKey().GetParticipant() != tt.wantPart {
				t.Fatalf("participant = %q, want %q", message.GetKey().GetParticipant(), tt.wantPart)
			}
		})
	}
}

func TestMarkChatAsReadEventCopiesRangeIdentity(t *testing.T) {
	chat, err := types.ParseJID("alice@s.whatsapp.net")
	if err != nil {
		t.Fatal(err)
	}
	id := "latest"
	fromMe := false
	event, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{
		JID: chat,
		Action: &waSyncAction.MarkChatAsReadAction{
			Read: proto.Bool(true),
			MessageRange: &waSyncAction.SyncActionMessageRange{
				LastMessageTimestamp: proto.Int64(42),
				Messages: []*waSyncAction.SyncActionMessage{{
					Key: &waCommon.MessageKey{RemoteJID: proto.String(chat.String()), ID: &id, FromMe: &fromMe},
				}},
			},
		},
	})
	if !ok || event.chat != chat.String() || event.messageID != id || event.timestamp != 42 || !event.read {
		t.Fatalf("event = %#v, ok = %v", event, ok)
	}
}

func TestMarkChatAsReadEventRejectsMissingRange(t *testing.T) {
	if _, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{}); ok {
		t.Fatal("missing range was accepted")
	}
}

func TestChatReadSyncRejectsInvalidInputBeforeSending(t *testing.T) {
	called := false
	err := sendChatReadSync(context.Background(), "", "latest", 42, false, "", func(context.Context, appstate.PatchInfo) error {
		called = true
		return nil
	})
	if err == nil || called {
		t.Fatalf("err = %v, called = %v, want validation error without send", err, called)
	}
}
