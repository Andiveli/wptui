package main

/*
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;
typedef struct {
	JID jid;
	const char* name;
	JID parent_jid;
	bool is_parent;
	bool is_joined;
	bool is_default_subgroup;
	uint8_t announcement;
	int64_t participant_count;
} CommunityEntry;
typedef struct {
	CommunityEntry* entries;
	uint32_t size;
	uint8_t status;
} GetCommunitiesResult;
*/
import "C"

import (
	"context"
	"unsafe"

	"go.mau.fi/whatsmeow/types"
)

type communityLookupClient interface {
	GetJoinedGroups(context.Context) ([]*types.GroupInfo, error)
	GetSubGroups(context.Context, types.JID) ([]*types.GroupLinkTarget, error)
}

type communityEntry struct {
	jid               types.JID
	name              string
	parent            types.JID
	isParent          bool
	joined            bool
	isDefaultSubGroup bool
	isAnnounce        *bool
	participantCount  *int
}

func lookupCommunityEntries(ctx context.Context, bridgeClient communityLookupClient) ([]communityEntry, error) {
	groups, err := bridgeClient.GetJoinedGroups(ctx)
	if err != nil {
		return nil, err
	}

	joinedByJID := make(map[types.JID]*types.GroupInfo, len(groups))
	roots := make([]*types.GroupInfo, 0, len(groups))
	seenRoots := make(map[types.JID]struct{}, len(groups))
	for _, group := range groups {
		if group == nil {
			continue
		}
		joinedByJID[group.JID] = group
		if group.IsParent {
			if _, seen := seenRoots[group.JID]; !seen {
				seenRoots[group.JID] = struct{}{}
				roots = append(roots, group)
			}
		}
	}

	entries := make([]communityEntry, 0, len(roots))
	entryIndex := make(map[types.JID]int, len(roots))
	for _, root := range roots {
		appendCommunityEntry(&entries, entryIndex, communityEntryFromGroup(root))

		targets, err := bridgeClient.GetSubGroups(ctx, root.JID)
		if err != nil {
			return nil, err
		}
		for _, target := range targets {
			if target == nil {
				continue
			}
			entry := communityEntry{
				jid:               target.JID,
				name:              target.GroupName.Name,
				parent:            root.JID,
				isDefaultSubGroup: target.IsDefaultSubGroup,
			}
			if joined := joinedByJID[target.JID]; joined != nil {
				entry = communityEntryFromGroup(joined)
			}
			appendCommunityEntry(&entries, entryIndex, entry)
		}
	}
	return entries, nil
}

func communityEntryFromGroup(group *types.GroupInfo) communityEntry {
	isAnnounce := group.IsAnnounce
	participantCount := group.ParticipantCount
	return communityEntry{
		jid:               group.JID,
		name:              group.GroupName.Name,
		parent:            group.LinkedParentJID,
		isParent:          group.IsParent,
		joined:            true,
		isDefaultSubGroup: group.IsDefaultSubGroup,
		isAnnounce:        &isAnnounce,
		participantCount:  &participantCount,
	}
}

func appendCommunityEntry(entries *[]communityEntry, indexes map[types.JID]int, entry communityEntry) {
	if index, exists := indexes[entry.jid]; exists {
		if entry.joined && !(*entries)[index].joined {
			(*entries)[index] = entry
		}
		return
	}
	indexes[entry.jid] = len(*entries)
	*entries = append(*entries, entry)
}

//export C_GetCommunities
func C_GetCommunities() C.GetCommunitiesResult {
	clientSnapshot := lifecycleState.clientSnapshot()
	entries, err := lookupCommunityEntries(context.Background(), clientSnapshot)
	if err != nil {
		LOG_WARN("Could not load communities: %v", err)
		return C.GetCommunitiesResult{status: 1}
	}
	return communityEntriesToC(entries)
}

// C_GetCommunities transfers ownership of entries and every pointed-to string
// to the caller. The caller must invoke C_FreeCommunities exactly once for
// every returned result, including empty and error results. C_FreeCommunities
// is nil-safe, but repeating it for the same non-nil result is not supported.
//
//export C_FreeCommunities
func C_FreeCommunities(result C.GetCommunitiesResult) {
	if result.entries == nil {
		return
	}
	freeCommunityEntries(unsafe.Slice(result.entries, int(result.size)))
	C.free(unsafe.Pointer(result.entries))
}

// Community announcement uses a stable u8 encoding: 0 unknown, 1 no, 2 yes.
// Community participant count uses -1 for unknown and a non-negative int64_t
// for a known count.
const (
	communityAnnouncementUnknown = 0
	communityAnnouncementNo      = 1
	communityAnnouncementYes     = 2
)

func communityAnnouncementCode(announce *bool) C.uint8_t {
	if announce == nil {
		return C.uint8_t(communityAnnouncementUnknown)
	}
	if *announce {
		return C.uint8_t(communityAnnouncementYes)
	}
	return C.uint8_t(communityAnnouncementNo)
}

func communityParticipantCountValue(participantCount *int) C.int64_t {
	if participantCount == nil {
		return C.int64_t(-1)
	}
	// Go int is at most 64 bits, so this conversion cannot truncate.
	return C.int64_t(*participantCount)
}

func communityEntriesToC(entries []communityEntry) C.GetCommunitiesResult {
	if len(entries) == 0 {
		return C.GetCommunitiesResult{}
	}
	cEntries := C.malloc(C.size_t(len(entries)) * C.size_t(unsafe.Sizeof(C.CommunityEntry{})))
	entryList := unsafe.Slice((*C.CommunityEntry)(cEntries), len(entries))
	for i, entry := range entries {
		entryList[i] = C.CommunityEntry{
			jid:                 jidToC(entry.jid),
			name:                C.CString(entry.name),
			parent_jid:          jidToC(entry.parent),
			is_parent:           C.bool(entry.isParent),
			is_joined:           C.bool(entry.joined),
			is_default_subgroup: C.bool(entry.isDefaultSubGroup),
			announcement:        communityAnnouncementCode(entry.isAnnounce),
			participant_count:   communityParticipantCountValue(entry.participantCount),
		}
	}
	return C.GetCommunitiesResult{entries: (*C.CommunityEntry)(cEntries), size: C.uint32_t(len(entries))}
}

func communityEntryStrings(entry C.CommunityEntry) (string, string, string, bool) {
	return C.GoString(entry.jid), C.GoString(entry.name), C.GoString(entry.parent_jid), bool(entry.is_parent)
}

func freeCommunityEntries(entries []C.CommunityEntry) {
	for _, entry := range entries {
		C.free(unsafe.Pointer(entry.jid))
		C.free(unsafe.Pointer(entry.name))
		C.free(unsafe.Pointer(entry.parent_jid))
	}
}

func communityEntryIncluded(isParent, parentEmpty bool) bool {
	return isParent || !parentEmpty
}
