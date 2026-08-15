package main

import (
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

func TestFreeCommunitiesNilResultIsSafe(t *testing.T) {
	freeCommunityEntries(nil)
}
