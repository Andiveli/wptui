package main

import (
	"context"
	"testing"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

func TestContactDisplayNameFallbackOrder(t *testing.T) {
	tests := []struct {
		name string
		info types.ContactInfo
		want string
	}{
		{name: "full name", info: types.ContactInfo{FullName: "Full", FirstName: "First"}, want: "Full"},
		{name: "first name", info: types.ContactInfo{FirstName: "First"}, want: "First"},
		{name: "push name", info: types.ContactInfo{PushName: "Push"}, want: "Push"},
		{name: "business name", info: types.ContactInfo{BusinessName: "Business"}, want: "Business"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := contactDisplayName(tt.info); got != tt.want {
				t.Fatalf("contactDisplayName() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestContactDisplayNamePrefersProfileOverNumericLocalName(t *testing.T) {
	contact := types.ContactInfo{FullName: "+1 555 123 4567", PushName: "Profile Name"}
	if got := contactDisplayName(contact); got != "Profile Name" {
		t.Fatalf("contactDisplayName() = %q, want profile name", got)
	}
}

func TestLoadContactEntriesUsesSelfProfileAndVerifiedPhoneFallback(t *testing.T) {
	self := types.JID{User: "15550000001", Server: types.DefaultUserServer}
	selfLID := types.JID{User: "9999", Server: types.HiddenUserServer}
	other := types.JID{User: "15550000002", Server: types.DefaultUserServer}
	otherLID := types.JID{User: "8888", Server: types.HiddenUserServer}
	contacts := groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
		self:     {FullName: "+1 555 000 0001"},
		otherLID: {},
	}}
	client := &whatsmeow.Client{Store: &store.Device{
		ID: &self, LID: selfLID, PushName: "My Profile", Contacts: contacts,
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{selfLID: self, otherLID: other},
			lidByPN: map[types.JID]types.JID{self: selfLID, other: otherLID},
		},
	}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[types.JID]string)
	for _, entry := range entries {
		got[entry.jid] = entry.name
	}
	if got[self] != "My Profile" || got[selfLID] != "My Profile" {
		t.Fatalf("self aliases = %q, %q, want profile name", got[self], got[selfLID])
	}
	if got[otherLID] != other.User || got[other] != other.User {
		t.Fatalf("verified aliases = %q, %q, want verified phone %q", got[otherLID], got[other], other.User)
	}
}

func TestContactAliasesForClientReadsSavedMappingWithoutGroupQuery(t *testing.T) {
	pn := types.NewJID("15550000002", types.DefaultUserServer)
	lid := types.NewJID("8888", types.HiddenUserServer)
	client := &whatsmeow.Client{Store: &store.Device{
		Contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
			pn: {FullName: "Saved Name"},
		}},
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	entries := contactAliasesForClient(client)
	got := make(map[types.JID]string)
	for _, entry := range entries {
		got[entry.jid] = entry.name
	}
	if got[pn] != "Saved Name" || got[lid] != "Saved Name" {
		t.Fatalf("contact-only aliases = %q, %q, want saved name for PN and LID", got[pn], got[lid])
	}
}

func TestLoadContactEntriesSavedNameWinsOverMappedLIDPhone(t *testing.T) {
	pn := types.NewJID("15550000002", types.DefaultUserServer)
	lid := types.NewJID("8888", types.HiddenUserServer)
	client := &whatsmeow.Client{Store: &store.Device{
		Contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
			pn: {FullName: "Saved Name"}, lid: {},
		}},
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	for attempt := 0; attempt < 10; attempt++ {
		entries, err := loadContactEntries(context.Background(), client)
		if err != nil {
			t.Fatal(err)
		}
		got := make(map[types.JID]string)
		for _, entry := range entries {
			got[entry.jid] = entry.name
		}
		if got[pn] != "Saved Name" || got[lid] != "Saved Name" {
			t.Fatalf("aliases = %q, %q, want saved name", got[pn], got[lid])
		}
	}
}

func TestLoadContactEntriesDoesNotReplaceSavedSelfAliasWithPhone(t *testing.T) {
	pn := types.NewJID("15550000001", types.DefaultUserServer)
	lid := types.NewJID("9999", types.HiddenUserServer)
	client := &whatsmeow.Client{Store: &store.Device{
		ID: &pn, LID: lid,
		Contacts: parityContactStore{contacts: map[types.JID]types.ContactInfo{
			pn: {FullName: "Saved Self Name"},
		}},
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[types.JID]string)
	for _, entry := range entries {
		got[entry.jid] = entry.name
	}
	if got[pn] != "Saved Self Name" || got[lid] != "Saved Self Name" {
		t.Fatalf("self aliases = %q, %q, want saved self name", got[pn], got[lid])
	}
}

func TestLoadContactEntriesSelfVerifiedPhoneWhenNoName(t *testing.T) {
	pn := types.NewJID("15550000001", types.DefaultUserServer)
	lid := types.NewJID("9999", types.HiddenUserServer)
	client := &whatsmeow.Client{Store: &store.Device{ID: &pn, LID: lid}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[types.JID]string)
	for _, entry := range entries {
		got[entry.jid] = entry.name
	}
	if got[pn] != pn.User || got[lid] != pn.User {
		t.Fatalf("self aliases = %q, %q, want verified phone %q", got[pn], got[lid], pn.User)
	}
}

func TestLoadContactEntriesDoesNotInventPhoneForUnmappedLID(t *testing.T) {
	lid := types.JID{User: "99887766", Server: types.HiddenUserServer}
	client := &whatsmeow.Client{Store: &store.Device{Contacts: groupParticipantContacts{
		contacts: map[types.JID]types.ContactInfo{lid: {}},
	}}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("unmapped LID entries = %#v, want none", entries)
	}
}

func TestLoadContactEntriesDoesNotExportNumericNameForUnmappedLID(t *testing.T) {
	lid := types.JID{User: "99887766", Server: types.HiddenUserServer}
	client := &whatsmeow.Client{Store: &store.Device{Contacts: groupParticipantContacts{
		contacts: map[types.JID]types.ContactInfo{lid: {FullName: lid.User}},
	}}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 0 {
		t.Fatalf("unmapped numeric LID entries = %#v, want none", entries)
	}
}

func TestLoadContactEntriesNumericLIDUsesVerifiedPN(t *testing.T) {
	lid := types.JID{User: "99887766", Server: types.HiddenUserServer}
	pn := types.JID{User: "15551234567", Server: types.DefaultUserServer}
	client := &whatsmeow.Client{Store: &store.Device{
		Contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
			lid: {FullName: lid.User},
		}},
		LIDs: participantIdentityLIDStore{pnByLID: map[types.JID]types.JID{lid: pn}},
	}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[types.JID]string)
	for _, entry := range entries {
		got[entry.jid] = entry.name
	}
	if got[lid] != pn.User || got[pn] != pn.User {
		t.Fatalf("mapped numeric LID aliases = %q, %q, want verified PN %q", got[lid], got[pn], pn.User)
	}
}

func TestLoadContactEntriesPNPlaceholderCannotBecomeMappedLIDPhone(t *testing.T) {
	pn := types.JID{User: "15551234567", Server: types.DefaultUserServer}
	lid := types.JID{User: "99887766", Server: types.HiddenUserServer}
	client := &whatsmeow.Client{Store: &store.Device{
		Contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
			pn: {FullName: lid.User},
		}},
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		t.Fatal(err)
	}
	got := make(map[types.JID]string)
	for _, entry := range entries {
		got[entry.jid] = entry.name
	}
	if got[pn] != pn.User || got[lid] != pn.User {
		t.Fatalf("placeholder aliases = %q, %q, want verified PN %q", got[pn], got[lid], pn.User)
	}
}

func TestContactJIDsIncludesVerifiedPNLIDAndADAliases(t *testing.T) {
	pn := types.JID{User: "141270097854639", Server: types.DefaultUserServer}
	lid := types.JID{User: "269595130773675", Server: types.HiddenUserServer}
	adPN := pn
	adPN.RawAgent = 1
	adPN.Device = 2

	got := contactJIDs(context.Background(), adPN, mentionLIDStore{pn: pn, lid: lid})
	want := []types.JID{adPN, pn, lid}
	if len(got) != len(want) {
		t.Fatalf("contact aliases = %#v, want %#v", got, want)
	}
	for index, alias := range want {
		if got[index] != alias {
			t.Fatalf("contact alias %d = %v, want %v", index, got[index], alias)
		}
	}
}

func TestConnectedSelfContactsExportOnlyAuthenticatedNamedAliases(t *testing.T) {
	self := types.NewJID("15550000001", types.DefaultUserServer)
	lid := types.NewJID("9999", types.HiddenUserServer)
	other := types.NewJID("15550000002", types.DefaultUserServer)
	for _, tt := range []struct {
		name    string
		profile string
		want    string
	}{
		{name: "profile available", profile: "Ada", want: "Ada"},
		{name: "numeric profile ignored", profile: "+1 555 000 0001"},
	} {
		t.Run(tt.name, func(t *testing.T) {
			client := &whatsmeow.Client{Store: &store.Device{
				ID: &self, LID: lid, PushName: tt.profile,
				Contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
					self:  {FullName: "+1 555 000 0001"},
					other: {FullName: "Other Saved Name"},
				}},
			}}
			result := contactEntriesToC(selfMentionEntries(context.Background(), client))
			defer C_FreeContacts(result)
			if tt.want == "" {
				if result.size != 0 {
					t.Fatalf("numeric profile exported %d entries, want none", result.size)
				}
				return
			}
			if result.size != 2 {
				t.Fatalf("self entries = %d, want PN and LID only", result.size)
			}
			seen := make(map[string]bool)
			for _, entry := range unsafe.Slice(result.entries, int(result.size)) {
				jid, name := contactEntryStrings(entry)
				if name != tt.want {
					t.Fatalf("self name = %q, want %q", name, tt.want)
				}
				seen[jid] = true
			}
			if !seen[self.String()] || !seen[lid.String()] || seen[other.String()] {
				t.Fatalf("exported JID set is not restricted to authenticated self aliases")
			}
		})
	}
}

func TestContactEntriesToCPreservesOrderAndEmptyOwnership(t *testing.T) {
	entries := []contactEntry{
		{jid: types.JID{User: "111", Server: types.DefaultUserServer}, name: "First"},
		{jid: types.JID{User: "111", Server: types.HiddenUserServer}, name: "First"},
		{jid: types.JID{User: "222", Server: types.GroupServer}, name: "Group"},
	}
	result := contactEntriesToC(entries)
	if result.size != 3 || result.entries == nil {
		t.Fatalf("result = (%p, %d), want three allocated entries", result.entries, result.size)
	}
	cEntries := unsafe.Slice(result.entries, 3)
	for i, want := range []struct{ jid, name string }{{"111@s.whatsapp.net", "First"}, {"111@lid", "First"}, {"222@g.us", "Group"}} {
		jid, name := contactEntryStrings(cEntries[i])
		if jid != want.jid {
			t.Errorf("entry %d jid = %q, want %q", i, jid, want.jid)
		}
		if name != want.name {
			t.Errorf("entry %d name = %q, want %q", i, name, want.name)
		}
	}
	C_FreeContacts(result)
	if empty := contactEntriesToC(nil); empty.entries != nil || empty.size != 0 {
		t.Fatalf("empty result = (%p, %d), want nil and zero", empty.entries, empty.size)
	} else {
		C_FreeContacts(empty)
	}
}
