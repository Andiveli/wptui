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

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

type communityEntry struct {
	jid      types.JID
	name     string
	parent   types.JID
	isParent bool
}

func lookupCommunityEntries(ctx context.Context, bridgeClient *whatsmeow.Client) ([]communityEntry, error) {
	groups, err := bridgeClient.GetJoinedGroups(ctx)
	if err != nil {
		return nil, err
	}
	entries := make([]communityEntry, 0, len(groups))
	for _, group := range groups {
		parent := group.LinkedParentJID
		if !communityEntryIncluded(group.IsParent, group.LinkedParentJID.IsEmpty()) {
			continue
		}
		entries = append(entries, communityEntry{
			jid:      group.JID,
			name:     group.GroupName.Name,
			parent:   parent,
			isParent: group.IsParent,
		})
	}
	return entries, nil
}

//export C_GetCommunities
func C_GetCommunities() C.GetCommunitiesResult {
	entries, err := lookupCommunityEntries(context.Background(), client)
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

func communityEntriesToC(entries []communityEntry) C.GetCommunitiesResult {
	if len(entries) == 0 {
		return C.GetCommunitiesResult{}
	}
	cEntries := C.malloc(C.size_t(len(entries)) * C.size_t(unsafe.Sizeof(C.CommunityEntry{})))
	entryList := unsafe.Slice((*C.CommunityEntry)(cEntries), len(entries))
	for i, entry := range entries {
		entryList[i] = C.CommunityEntry{
			jid:        jidToC(entry.jid),
			name:       C.CString(entry.name),
			parent_jid: jidToC(entry.parent),
			is_parent:  C.bool(entry.isParent),
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
