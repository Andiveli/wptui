package main

import (
	"context"
	"errors"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type chatIdentityLIDStore struct {
	pn types.JID
}

func (s chatIdentityLIDStore) PutManyLIDMappings(context.Context, []store.LIDMapping) error {
	return nil
}

func (s chatIdentityLIDStore) PutLIDMapping(context.Context, types.JID, types.JID) error {
	return nil
}

func (s chatIdentityLIDStore) GetPNForLID(context.Context, types.JID) (types.JID, error) {
	return s.pn, nil
}

func (s chatIdentityLIDStore) GetLIDForPN(context.Context, types.JID) (types.JID, error) {
	return types.JID{}, errors.New("not implemented")
}

func (s chatIdentityLIDStore) GetManyLIDsForPNs(context.Context, []types.JID) (map[types.JID]types.JID, error) {
	return nil, errors.New("not implemented")
}

func TestGetChatIdPreservesChatIdentityKinds(t *testing.T) {
	private := types.NewJID("15551234567", types.DefaultUserServer)
	device := private
	device.Device = 2
	tests := []struct {
		name   string
		chat   types.JID
		sender *types.JID
		want   string
	}{
		{name: "phone number", chat: private, want: "15551234567@s.whatsapp.net"},
		{name: "LID maps to phone number", chat: types.NewJID("alice", types.HiddenUserServer), want: "15551234567@s.whatsapp.net"},
		{name: "device keeps user server", chat: device, want: "15551234567@s.whatsapp.net"},
		{name: "group remains group", chat: types.NewJID("12345-678", types.GroupServer), want: "12345-678@g.us"},
		{name: "newsletter remains newsletter", chat: types.NewJID("channel", types.NewsletterServer), want: "channel@newsletter"},
		{name: "status remains broadcast", chat: types.NewJID("status", types.BroadcastServer), want: "status@broadcast"},
		{name: "broadcast uses sender", chat: types.NewJID("updates", types.BroadcastServer), sender: &private, want: "15551234567@s.whatsapp.net"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			client := &whatsmeow.Client{Store: &store.Device{LIDs: chatIdentityLIDStore{pn: private}}}
			if tt.chat.Server != types.HiddenUserServer {
				client = nil
			}
			if got := GetChatId(client, &tt.chat, tt.sender); got != tt.want {
				t.Fatalf("GetChatId() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestGetChatIdRejectsMissingChat(t *testing.T) {
	if got := GetChatId(nil, nil, nil); got != "" {
		t.Fatalf("GetChatId(nil) = %q, want empty", got)
	}
}

func TestGetUserIdPreservesGroupAndDirectIdentity(t *testing.T) {
	user := types.NewJID("alice", types.HiddenUserServer)
	group := types.NewJID("12345-678", types.GroupServer)

	if got := GetUserId(nil, &group, &user); got != "alice@lid" {
		t.Fatalf("group GetUserId() = %q, want %q", got, "alice@lid")
	}
	if got := GetUserId(nil, nil, &types.JID{User: "15551234567", Server: types.DefaultUserServer, Device: 2}); got != "15551234567@s.whatsapp.net" {
		t.Fatalf("direct device GetUserId() = %q, want %q", got, "15551234567@s.whatsapp.net")
	}
}

func TestIdentityHelpersRejectMissingUser(t *testing.T) {
	if got := GetUserId(nil, nil, nil); got != "" {
		t.Fatalf("GetUserId(nil) = %q, want empty", got)
	}
}

func TestStrFromJidDoesNotMapIdentity(t *testing.T) {
	jid := types.NewJID("alice", types.DefaultUserServer)
	jid.Device = 9
	if got := StrFromJid(jid); got != "alice@s.whatsapp.net" {
		t.Fatalf("StrFromJid() = %q, want %q", got, "alice@s.whatsapp.net")
	}
}
