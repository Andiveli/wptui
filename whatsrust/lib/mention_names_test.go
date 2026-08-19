package main

import (
	"context"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type mentionContactStore struct{}

func (mentionContactStore) PutPushName(context.Context, types.JID, string) (bool, string, error) {
	return false, "", nil
}

func (mentionContactStore) PutBusinessName(context.Context, types.JID, string) (bool, string, error) {
	return false, "", nil
}

func (mentionContactStore) PutContactName(context.Context, types.JID, string, string) error {
	return nil
}

func (mentionContactStore) PutAllContactNames(context.Context, []store.ContactEntry) error {
	return nil
}

func (mentionContactStore) PutManyRedactedPhones(context.Context, []store.RedactedPhoneEntry) error {
	return nil
}

func (mentionContactStore) GetContact(context.Context, types.JID) (types.ContactInfo, error) {
	return types.ContactInfo{}, nil
}

func (mentionContactStore) GetAllContacts(context.Context) (map[types.JID]types.ContactInfo, error) {
	return map[types.JID]types.ContactInfo{
		{User: "123", Server: types.DefaultUserServer}: {FullName: "Alice"},
	}, nil
}

func TestTextWithMentionNamesResolvesSynchronizedMessages(t *testing.T) {
	previousClient := client
	client = &whatsmeow.Client{Store: &store.Device{Contacts: mentionContactStore{}}}
	t.Cleanup(func() { client = previousClient })

	contextInfo := &waE2E.ContextInfo{MentionedJID: []string{"123@s.whatsapp.net"}}
	if got := textWithMentionNames("hello @123", contextInfo); got != "hello @Alice" {
		t.Fatalf("textWithMentionNames() = %q, want synchronized mention resolved", got)
	}
}

func TestReplaceMentionedNames(t *testing.T) {
	tests := []struct {
		name      string
		text      string
		mentioned []string
		contacts  []contactEntry
		want      string
	}{
		{
			name:     "no context preserves text",
			text:     "hello @123 and @456",
			contacts: []contactEntry{{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "Alice"}},
			want:     "hello @123 and @456",
		},
		{
			name:      "multiple and repeated mentions replace only matching tokens",
			text:      "@123 hi @456 @123 @999 @1234",
			mentioned: []string{"123@s.whatsapp.net", "456@lid"},
			contacts: []contactEntry{
				{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "Alice"},
				{jid: types.JID{User: "456", Server: types.HiddenUserServer}, name: "Bob"},
			},
			want: "@Alice hi @Bob @Alice @999 @1234",
		},
		{
			name:      "PN and LID aliases resolve to the same contact name",
			text:      "Replying to @777",
			mentioned: []string{"777@lid"},
			contacts: []contactEntry{
				{jid: types.JID{User: "777", Server: types.DefaultUserServer}, name: "Carol"},
				{jid: types.JID{User: "777", Server: types.HiddenUserServer}, name: "Carol"},
			},
			want: "Replying to @Carol",
		},
		{
			name:      "malformed metadata and unusable names preserve numeric tokens",
			text:      "@bad @888 @999",
			mentioned: []string{"not-a-jid", "888@s.whatsapp.net", "999@s.whatsapp.net"},
			contacts: []contactEntry{
				{jid: types.JID{User: "888", Server: types.DefaultUserServer}, name: "  "},
			},
			want: "@bad @888 @999",
		},
		{
			name:      "tokens require boundaries and preserve punctuation",
			text:      "abc@123 @123, @123.",
			mentioned: []string{"123@s.whatsapp.net"},
			contacts: []contactEntry{
				{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "Alice"},
			},
			want: "abc@123 @Alice, @Alice.",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := replaceMentionedNames(tt.text, tt.mentioned, tt.contacts); got != tt.want {
				t.Fatalf("replaceMentionedNames() = %q, want %q", got, tt.want)
			}
		})
	}
}
