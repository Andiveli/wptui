package main

import (
	"context"
	"errors"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type groupParticipantContacts struct {
	contacts map[types.JID]types.ContactInfo
	err      error
}

func (s groupParticipantContacts) PutPushName(context.Context, types.JID, string) (bool, string, error) {
	return false, "", nil
}

func (s groupParticipantContacts) PutBusinessName(context.Context, types.JID, string) (bool, string, error) {
	return false, "", nil
}

func (s groupParticipantContacts) PutContactName(context.Context, types.JID, string, string) error {
	return nil
}

func (s groupParticipantContacts) PutAllContactNames(context.Context, []store.ContactEntry) error {
	return nil
}

func (s groupParticipantContacts) PutManyRedactedPhones(context.Context, []store.RedactedPhoneEntry) error {
	return nil
}

func (s groupParticipantContacts) GetContact(_ context.Context, jid types.JID) (types.ContactInfo, error) {
	if s.err != nil {
		return types.ContactInfo{}, s.err
	}
	return s.contacts[jid], nil
}

func (s groupParticipantContacts) GetAllContacts(context.Context) (map[types.JID]types.ContactInfo, error) {
	return s.contacts, nil
}

var _ store.ContactStore = groupParticipantContacts{}

func TestGroupParticipantNameFallsBackToProtocolIdentity(t *testing.T) {
	participant := types.GroupParticipant{
		JID:         types.JID{User: "123", Server: types.DefaultUserServer},
		PhoneNumber: types.JID{User: "456", Server: types.DefaultUserServer},
	}
	if got := groupParticipantName(context.Background(), participant, groupParticipantContacts{}); got != "456" {
		t.Fatalf("groupParticipantName() = %q, want phone-number fallback", got)
	}
}

func TestGroupParticipantNameUsesPlainSavedAndPushNamesBeforeNumericFallback(t *testing.T) {
	phone := types.JID{User: "123", Server: types.DefaultUserServer}
	tests := []struct {
		name        string
		participant types.GroupParticipant
		contacts    groupParticipantContacts
		want        string
	}{
		{
			name:        "push name when display is unavailable",
			participant: types.GroupParticipant{JID: phone},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				phone: {PushName: "Profile"},
			}},
			want: "Profile",
		},
		{
			name: "legacy helper prefix is not rendered",
			participant: types.GroupParticipant{
				JID:         phone,
				DisplayName: "~ Profile",
			},
			want: "Profile",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := groupParticipantName(context.Background(), tt.participant, tt.contacts); got != tt.want {
				t.Fatalf("groupParticipantName() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestDeduplicateGroupParticipantsUsesExplicitAliasesOnly(t *testing.T) {
	pn := func(user string) types.JID {
		return types.JID{User: user, Server: types.DefaultUserServer}
	}
	lid := func(user string) types.JID {
		return types.JID{User: user, Server: types.HiddenUserServer}
	}
	participants := []types.GroupParticipant{
		{JID: pn("123"), PhoneNumber: pn("123"), DisplayName: "Alice"},
		{JID: lid("999"), PhoneNumber: pn("123"), DisplayName: "Alice alias"},
		{JID: lid("123"), PhoneNumber: lid("123"), DisplayName: "Different person"},
	}
	got := deduplicateGroupParticipants(context.Background(), participants, nil)
	if len(got) != 2 {
		t.Fatalf("deduplicated participants = %d, want two identities: %#v", len(got), got)
	}
	if got[0].DisplayName != "Alice" || got[1].DisplayName != "Different person" {
		t.Fatalf("deduplicated participants = %#v, want aliases merged without collapsing distinct identities", got)
	}
}

func TestGroupParticipantNamePrefersSelfLocalNameThenPushNameThenNumeric(t *testing.T) {
	previousClient := client
	t.Cleanup(func() { client = previousClient })

	self := types.NewJID("123", types.DefaultUserServer)
	client = &whatsmeow.Client{Store: &store.Device{
		ID:       &self,
		PushName: "Connected Profile",
	}}
	participant := types.GroupParticipant{JID: self, DisplayName: "Other Display"}

	for name, tt := range map[string]struct {
		contacts store.ContactStore
		want     string
	}{
		"local full name": {
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				self: {FullName: "Saved Full Name", FirstName: "Saved First Name"},
			}},
			want: "Saved Full Name",
		},
		"local first name": {
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				self: {FirstName: "Saved First Name"},
			}},
			want: "Saved First Name",
		},
		"store push name": {want: "Connected Profile"},
	} {
		t.Run(name, func(t *testing.T) {
			if got := groupParticipantName(context.Background(), participant, tt.contacts); got != tt.want {
				t.Fatalf("groupParticipantName() = %q, want %q", got, tt.want)
			}
		})
	}

	client.Store.PushName = ""
	if got := groupParticipantName(context.Background(), participant, nil); got != "123" {
		t.Fatalf("groupParticipantName() without local or push name = %q, want numeric fallback", got)
	}

	other := types.GroupParticipant{JID: types.NewJID("456", types.DefaultUserServer)}
	client.Store.PushName = "Connected Profile"
	if got := groupParticipantName(context.Background(), other, nil); got == client.Store.PushName {
		t.Fatalf("groupParticipantName() assigned self push name to another participant: %q", got)
	}
}

func TestGroupParticipantPickerExcludesSelfAndRetainsAllOtherMembers(t *testing.T) {
	previousClient := client
	t.Cleanup(func() { client = previousClient })

	self := types.NewJID("123", types.DefaultUserServer)
	client = &whatsmeow.Client{Store: &store.Device{ID: &self}}
	participants := []types.GroupParticipant{
		{JID: self},
		{JID: types.NewJID("456", types.DefaultUserServer)},
		{JID: types.NewJID("789", types.HiddenUserServer)},
	}
	got := excludeSelfGroupParticipants(context.Background(), participants)
	if len(got) != 2 {
		t.Fatalf("picker candidates = %d, want two non-self members", len(got))
	}
	for _, participant := range got {
		if participantMatchesSelf(client, participant) {
			t.Fatalf("picker retained self participant %#v", participant)
		}
	}
	if got[0].JID.User != "456" || got[1].JID.User != "789" {
		t.Fatalf("picker candidates = %#v, want every non-self member in order", got)
	}
}

func TestGroupParticipantNamePrefersLocallySavedContactName(t *testing.T) {
	phone := types.JID{User: "123", Server: types.DefaultUserServer}
	lid := types.JID{User: "456", Server: types.HiddenUserServer}
	tests := []struct {
		name        string
		participant types.GroupParticipant
		contacts    groupParticipantContacts
		want        string
	}{
		{
			name: "full name wins over participant display name",
			participant: types.GroupParticipant{
				JID:         phone,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				phone: {FullName: "Saved Full Name", FirstName: "Saved First Name"},
			}},
			want: "Saved Full Name",
		},
		{
			name: "first name wins when full name is empty",
			participant: types.GroupParticipant{
				JID:         phone,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				phone: {FirstName: "Saved First Name"},
			}},
			want: "Saved First Name",
		},
		{
			name: "phone lookup can find the saved name",
			participant: types.GroupParticipant{
				JID:         lid,
				PhoneNumber: phone,
				LID:         lid,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				phone: {FullName: "Saved As"},
			}},
			want: "Saved As",
		},
		{
			name: "push name alone does not replace participant display name",
			participant: types.GroupParticipant{
				JID:         phone,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				phone: {PushName: "Self Configured"},
			}},
			want: "WhatsApp Profile",
		},
		{
			name: "participant display name is the fallback without a saved name",
			participant: types.GroupParticipant{
				JID:         phone,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				phone: {},
			}},
			want: "WhatsApp Profile",
		},
		{
			name: "participant display name is the fallback when lookup fails",
			participant: types.GroupParticipant{
				JID:         phone,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{err: errors.New("contact lookup failed")},
			want:     "WhatsApp Profile",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := groupParticipantName(context.Background(), tt.participant, tt.contacts); got != tt.want {
				t.Fatalf("groupParticipantName() = %q, want %q", got, tt.want)
			}
		})
	}
}
