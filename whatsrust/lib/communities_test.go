package main

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"unsafe"

	"go.mau.fi/whatsmeow/types"
)

func TestCommunityEntryIncludedDistinguishesRootsAndLinkedGroups(t *testing.T) {
	tests := []struct {
		name        string
		isParent    bool
		parentEmpty bool
		want        bool
	}{
		{name: "community root", isParent: true, parentEmpty: true, want: true},
		{name: "linked group", parentEmpty: false, want: true},
		{name: "ordinary group", parentEmpty: true, want: false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := communityEntryIncluded(tt.isParent, tt.parentEmpty); got != tt.want {
				t.Fatalf("communityEntryIncluded(%t, %t) = %t, want %t", tt.isParent, tt.parentEmpty, got, tt.want)
			}
		})
	}
}

func TestCommunityEntriesToCPreservesOrderAndHierarchy(t *testing.T) {
	entries := []communityEntry{
		{jid: types.NewJID("root", types.GroupServer), name: "Root", isParent: true},
		{jid: types.NewJID("child", types.GroupServer), name: "Child", parent: types.NewJID("root", types.GroupServer)},
	}
	result := communityEntriesToC(entries)
	if result.entries == nil || result.size != 2 {
		t.Fatalf("result = (%p, %d), want two allocated entries", result.entries, result.size)
	}
	cEntries := unsafe.Slice(result.entries, 2)
	jid, name, parent, isParent := communityEntryStrings(cEntries[0])
	if jid != "root@g.us" || name != "Root" || parent != "" || !isParent {
		t.Fatalf("root = (%q, %q, %q, %t), want root and parent", jid, name, parent, isParent)
	}
	jid, name, parent, isParent = communityEntryStrings(cEntries[1])
	if jid != "child@g.us" || name != "Child" || parent != "root@g.us" || isParent {
		t.Fatalf("child = (%q, %q, %q, %t)", jid, name, parent, isParent)
	}
	C_FreeCommunities(result)
	if empty := communityEntriesToC(nil); empty.entries != nil || empty.size != 0 || empty.status != 0 {
		t.Fatalf("empty result = (%p, %d, %d), want nil, zero, zero", empty.entries, empty.size, empty.status)
	}
}

func TestFreeCommunitiesNilResultIsSafe(t *testing.T) { freeCommunityEntries(nil) }

type fakeCommunityLookup struct {
	joined                []*types.GroupInfo
	subs                  map[types.JID][]*types.GroupLinkTarget
	errs                  map[types.JID]error
	joinedCalls, subCalls int
}

func (f *fakeCommunityLookup) GetJoinedGroups(context.Context) ([]*types.GroupInfo, error) {
	f.joinedCalls++
	return f.joined, nil
}
func (f *fakeCommunityLookup) GetSubGroups(_ context.Context, jid types.JID) ([]*types.GroupLinkTarget, error) {
	f.subCalls++
	if err := f.errs[jid]; err != nil {
		return nil, err
	}
	return f.subs[jid], nil
}

func testGroup(id, name string, parent types.JID, root, def, announce bool, count int) *types.GroupInfo {
	return &types.GroupInfo{
		JID: types.NewJID(id, types.GroupServer), GroupName: types.GroupName{Name: name},
		GroupParent: types.GroupParent{IsParent: root}, GroupLinkedParent: types.GroupLinkedParent{LinkedParentJID: parent},
		GroupIsDefaultSub: types.GroupIsDefaultSub{IsDefaultSubGroup: def}, GroupAnnounce: types.GroupAnnounce{IsAnnounce: announce}, ParticipantCount: count,
	}
}
func testTarget(id, name string, def bool) *types.GroupLinkTarget {
	return &types.GroupLinkTarget{JID: types.NewJID(id, types.GroupServer), GroupName: types.GroupName{Name: name}, GroupIsDefaultSub: types.GroupIsDefaultSub{IsDefaultSubGroup: def}}
}
func entryFor(entries []communityEntry, jid types.JID) *communityEntry {
	for i := range entries {
		if entries[i].jid == jid {
			return &entries[i]
		}
	}
	return nil
}

func TestLookupCommunityEntriesFetchesEachRootOnceAndEmitsTargets(t *testing.T) {
	one, two := types.NewJID("one", types.GroupServer), types.NewJID("two", types.GroupServer)
	lookup := &fakeCommunityLookup{
		joined: []*types.GroupInfo{testGroup("one", "One", types.JID{}, true, false, false, 1), testGroup("ordinary", "Ordinary", types.JID{}, false, false, false, 2), testGroup("two", "Two", types.JID{}, true, false, false, 3)},
		subs:   map[types.JID][]*types.GroupLinkTarget{one: {testTarget("child", "Child", true)}, two: {testTarget("other", "Other", false)}}, errs: map[types.JID]error{},
	}
	entries, err := lookupCommunityEntries(context.Background(), lookup)
	if err != nil || lookup.joinedCalls != 1 || lookup.subCalls != 2 {
		t.Fatalf("lookup = (%v, joined calls %d, subgroup calls %d), want success, 1, 2", err, lookup.joinedCalls, lookup.subCalls)
	}
	want := []types.JID{one, types.NewJID("child", types.GroupServer), two, types.NewJID("other", types.GroupServer)}
	got := make([]types.JID, len(entries))
	for i, entry := range entries {
		got[i] = entry.jid
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("JIDs = %v, want %v", got, want)
	}
	if child := entryFor(entries, want[1]); child == nil || !child.isDefaultSubGroup {
		t.Fatalf("child = %#v, want default subgroup", child)
	}
}

func TestLookupCommunityEntriesMergesJoinedMetadataAndAnnouncementState(t *testing.T) {
	for _, announce := range []bool{true, false} {
		t.Run(map[bool]string{true: "yes", false: "no"}[announce], func(t *testing.T) {
			root, child := types.NewJID("root", types.GroupServer), types.NewJID("child", types.GroupServer)
			lookup := &fakeCommunityLookup{
				joined: []*types.GroupInfo{testGroup("root", "Root", types.JID{}, true, false, false, 1), testGroup("child", "Joined", root, false, true, announce, 7)},
				subs:   map[types.JID][]*types.GroupLinkTarget{root: {testTarget("child", "Stale", false)}}, errs: map[types.JID]error{},
			}
			entries, err := lookupCommunityEntries(context.Background(), lookup)
			entry := entryFor(entries, child)
			if err != nil || entry == nil || entry.name != "Joined" || !entry.joined || !entry.isDefaultSubGroup || entry.parent != root || entry.isAnnounce == nil || *entry.isAnnounce != announce || entry.participantCount == nil || *entry.participantCount != 7 {
				t.Fatalf("entry = %#v, error = %v, want joined enriched metadata", entry, err)
			}
		})
	}
}

func TestLookupCommunityEntriesLeavesNonjoinedMetadataUnknown(t *testing.T) {
	root, child := types.NewJID("root", types.GroupServer), types.NewJID("child", types.GroupServer)
	lookup := &fakeCommunityLookup{joined: []*types.GroupInfo{testGroup("root", "Root", types.JID{}, true, false, false, 1)}, subs: map[types.JID][]*types.GroupLinkTarget{root: {testTarget("child", "Child", true)}}, errs: map[types.JID]error{}}
	entries, err := lookupCommunityEntries(context.Background(), lookup)
	entry := entryFor(entries, child)
	if err != nil || entry == nil || entry.joined || !entry.isDefaultSubGroup || entry.isAnnounce != nil || entry.participantCount != nil {
		t.Fatalf("entry = %#v, error = %v, want nonjoined unknown metadata and default marker", entry, err)
	}
}

func TestLookupCommunityEntriesMergesDuplicateJIDs(t *testing.T) {
	root, child := types.NewJID("root", types.GroupServer), types.NewJID("child", types.GroupServer)
	lookup := &fakeCommunityLookup{joined: []*types.GroupInfo{testGroup("root", "Root", types.JID{}, true, false, false, 1), testGroup("child", "Joined", root, false, false, false, 5)}, subs: map[types.JID][]*types.GroupLinkTarget{root: {testTarget("child", "First", false), testTarget("child", "Duplicate", true)}}, errs: map[types.JID]error{}}
	entries, err := lookupCommunityEntries(context.Background(), lookup)
	entry := entryFor(entries, child)
	if err != nil || len(entries) != 2 || entry == nil || entry.name != "Joined" || entry.participantCount == nil || *entry.participantCount != 5 {
		t.Fatalf("entries = %#v, error = %v, want one joined entry for duplicate JID", entries, err)
	}
}

func TestLookupCommunityEntriesReturnsNoPartialSnapshotOnSubgroupFailure(t *testing.T) {
	two, failure := types.NewJID("two", types.GroupServer), errors.New("subgroup fetch failed")
	lookup := &fakeCommunityLookup{joined: []*types.GroupInfo{testGroup("one", "One", types.JID{}, true, false, false, 1), testGroup("two", "Two", types.JID{}, true, false, false, 2)}, subs: map[types.JID][]*types.GroupLinkTarget{}, errs: map[types.JID]error{two: failure}}
	entries, err := lookupCommunityEntries(context.Background(), lookup)
	if !errors.Is(err, failure) || entries != nil {
		t.Fatalf("lookup = (%#v, %v), want nil snapshot and subgroup error", entries, err)
	}
}
