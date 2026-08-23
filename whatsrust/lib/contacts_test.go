package main

import (
	"context"
	"testing"
	"unsafe"

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
	freeContactResult(result)
	if empty := contactEntriesToC(nil); empty.entries != nil || empty.size != 0 {
		t.Fatalf("empty result = (%p, %d), want nil and zero", empty.entries, empty.size)
	}
}
