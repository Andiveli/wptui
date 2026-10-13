package main

import (
	"context"
	"errors"
	"fmt"
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
	placeholder := types.NewJID("15550000004", types.DefaultUserServer)
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
				alice:       {FullName: "Alice"},
				bob:         {PushName: "Bob"},
				charlie:     {FirstName: "Charlie"},
				placeholder: {FullName: "15550000004", PushName: "Profile"},
				lid:         {FullName: "LID name"},
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

func TestSavedAliasLookupNotifiesOnlyAfterVerifiedServerMapping(t *testing.T) {
	pn := types.NewJID("15550000001", types.DefaultUserServer)
	other := types.NewJID("15550000002", types.DefaultUserServer)
	lid := types.NewJID("99887766", types.HiddenUserServer)
	for _, tc := range []struct {
		name       string
		responsePN types.JID
		queryErr   error
		wantNotify int
	}{
		{name: "confirmed requested alias", responsePN: pn, wantNotify: 1},
		{name: "unexpected PN is not this contact", responsePN: other},
		{name: "empty response"},
		{name: "network error", queryErr: errors.New("network unavailable")},
	} {
		t.Run(tc.name, func(t *testing.T) {
			lids := aliasSelectionLIDs{mapped: make(map[types.JID]types.JID)}
			calls, notified := 0, 0
			runSavedAliasLookup(context.Background(),
				map[types.JID]types.ContactInfo{pn: {FullName: "Alice"}}, lids,
				func(_ context.Context, phones []string) ([]types.IsOnWhatsAppResponse, error) {
					calls++
					if !slices.Equal(phones, []string{"+15550000001"}) {
						t.Errorf("queried phones differ from the saved PN")
					}
					if tc.queryErr != nil {
						return nil, tc.queryErr
					}
					if tc.responsePN.IsEmpty() {
						return nil, nil
					}
					lids.mapped[tc.responsePN] = lid // Fake the dependency's verified cache write.
					return []types.IsOnWhatsAppResponse{{JID: lid, PhoneNumber: tc.responsePN}}, nil
				}, func() { notified++ })
			if calls != 1 || notified != tc.wantNotify {
				t.Errorf("query calls=%d, refresh events=%d; want 1 and %d", calls, notified, tc.wantNotify)
			}
		})
	}
}

func TestSavedAliasLookupLimitsRequestsAndStopsOnCancellation(t *testing.T) {
	contacts := make(map[types.JID]types.ContactInfo)
	for index := range 17 {
		contacts[types.NewJID(fmt.Sprintf("1555000%04d", index), types.DefaultUserServer)] =
			types.ContactInfo{FirstName: "Saved"}
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	var batchSizes []int
	runSavedAliasLookup(ctx, contacts, aliasSelectionLIDs{},
		func(_ context.Context, phones []string) ([]types.IsOnWhatsAppResponse, error) {
			batchSizes = append(batchSizes, len(phones))
			cancel()
			return nil, nil
		}, func() { t.Error("canceled lookup must not signal a refresh") })
	if !slices.Equal(batchSizes, []int{16}) {
		t.Errorf("batch sizes after cancellation = %v, want [16]", batchSizes)
	}
}
