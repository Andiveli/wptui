package main

import (
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

func TestGetSelfIdReturnsConfiguredIdentity(t *testing.T) {
	client := &whatsmeow.Client{
		Store: &store.Device{ID: pointerToJID(types.NewJID("15551234567", types.DefaultUserServer))},
	}

	if got := GetSelfId(client); got != "15551234567@s.whatsapp.net" {
		t.Fatalf("GetSelfId() = %q, want %q", got, "15551234567@s.whatsapp.net")
	}
}

func TestGetSelfIdReturnsEmptyForMissingIdentity(t *testing.T) {
	tests := []struct {
		name   string
		client *whatsmeow.Client
	}{
		{name: "nil client"},
		{name: "nil store", client: &whatsmeow.Client{}},
		{name: "nil device id", client: &whatsmeow.Client{Store: &store.Device{}}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := GetSelfId(tt.client); got != "" {
				t.Fatalf("GetSelfId() = %q, want empty", got)
			}
		})
	}
}

func pointerToJID(jid types.JID) *types.JID {
	return &jid
}
