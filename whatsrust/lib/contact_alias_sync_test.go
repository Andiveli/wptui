package main

import (
	"context"
	"errors"
	"slices"
	"testing"

	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type aliasSelectionLIDs struct {
	store.LIDStore
	mapped map[types.JID]types.JID
	failed map[types.JID]bool
}

func (s aliasSelectionLIDs) GetLIDForPN(_ context.Context, pn types.JID) (types.JID, error) {
	if s.failed[pn] {
		return types.JID{}, errors.New("lookup unavailable")
	}
	return s.mapped[pn], nil
}

func TestSavedPNsMissingAliasSelectsOnlyUnmappedSavedContacts(t *testing.T) {
	alice := types.NewJID("15550000001", types.DefaultUserServer)
	bob := types.NewJID("15550000002", types.DefaultUserServer)
	charlie := types.NewJID("15550000003", types.DefaultUserServer)
	lid := types.NewJID("99887766", types.HiddenUserServer)
	for _, tc := range []struct {
		name     string
		contacts map[types.JID]types.ContactInfo
		mapped   map[types.JID]types.JID
		failed   map[types.JID]bool
		limit    int
		want     []types.JID
	}{
		{
			name: "saved PN without alias",
			contacts: map[types.JID]types.ContactInfo{
				alice: {FullName: "Alice"},
			},
			limit: 2,
			want:  []types.JID{alice},
		},
		{
			name: "skip profiles, LIDs, mapped and failed PNs",
			contacts: map[types.JID]types.ContactInfo{
				alice:   {FullName: "Alice"},
				bob:     {PushName: "Bob"},
				charlie: {FirstName: "Charlie"},
				lid:     {FullName: "LID name"},
			},
			mapped: map[types.JID]types.JID{alice: lid},
			failed: map[types.JID]bool{charlie: true},
			limit:  4,
		},
		{
			name: "deterministic cap",
			contacts: map[types.JID]types.ContactInfo{
				charlie: {FullName: "Charlie"},
				alice:   {FullName: "Alice"},
				bob:     {FirstName: "Bob"},
			},
			limit: 2,
			want:  []types.JID{alice, bob},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got := savedPNsMissingAlias(context.Background(), tc.contacts,
				aliasSelectionLIDs{mapped: tc.mapped, failed: tc.failed}, tc.limit)
			if !slices.Equal(got, tc.want) {
				t.Errorf("candidate selection = %v, want %v", got, tc.want)
			}
		})
	}
}
