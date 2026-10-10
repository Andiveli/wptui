package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;
typedef struct {
	JID jid;
	const char* name;
} ContactEntry;
typedef struct {
	ContactEntry* entries;
	uint32_t size;
} GetContactsResult;
*/
import "C"

import (
	"context"
	"sort"
	"strings"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type contactEntry struct {
	jid  types.JID
	name string
}

// Prefer a saved name, unless the phone stored in the address book is only
// a numeric placeholder for the contact's WhatsApp profile name.
func contactDisplayName(c types.ContactInfo) string {
	name, _ := contactNameCandidate(c)
	return name
}

// Higher ranks win when PN and LID contact records describe the same person.
func contactNameCandidate(c types.ContactInfo) (string, int) {
	var numericLocal string
	for _, candidate := range []string{c.FullName, c.FirstName} {
		if name := plainContactName(candidate); name != "" {
			if !phoneLikeName(name) {
				return name, 3
			}
			if numericLocal == "" {
				numericLocal = name
			}
		}
	}
	for _, candidate := range []string{c.PushName, c.BusinessName} {
		if name := plainContactName(candidate); name != "" && !phoneLikeName(name) {
			return name, 2
		}
	}
	if numericLocal != "" {
		return numericLocal, 1
	}
	return "", 0
}

func plainContactName(name string) string {
	name = strings.TrimSpace(name)
	for _, prefix := range []string{"~ ", "+ "} {
		if strings.HasPrefix(name, prefix) {
			name = strings.TrimSpace(strings.TrimPrefix(name, prefix))
			break
		}
	}
	user, server, hasServer := strings.Cut(name, "@")
	if hasServer && strings.EqualFold(server, types.HiddenUserServer) && !strings.ContainsAny(user, " \t\n\r") {
		return ""
	}
	return name
}

func lookupContactEntries(ctx context.Context, bridgeClient *whatsmeow.Client) []contactEntry {
	entries, err := loadContactEntries(ctx, bridgeClient)
	if err != nil {
		panic(err)
	}
	return entries
}

func lookupMentionContactEntries() []contactEntry {
	clientSnapshot := lifecycleState.clientSnapshot()
	entries, err := loadContactEntries(context.Background(), clientSnapshot)
	if err != nil {
		return nil
	}
	return entries
}

func loadContactEntries(ctx context.Context, bridgeClient *whatsmeow.Client) ([]contactEntry, error) {
	if bridgeClient == nil || bridgeClient.Store == nil {
		return nil, nil
	}
	var contacts map[types.JID]types.ContactInfo
	if bridgeClient.Store.Contacts != nil {
		var err error
		contacts, err = bridgeClient.Store.Contacts.GetAllContacts(ctx)
		if err != nil {
			return nil, err
		}
	}
	type rankedName struct {
		name string
		rank int
	}
	resolved := make(map[types.JID]rankedName, len(contacts))
	for jid, contact := range contacts {
		aliases := contactJIDs(ctx, jid, bridgeClient.Store.LIDs)
		name, rank := contactNameCandidate(contact)
		if name == "" {
			for _, alias := range aliases {
				if alias.Server == types.DefaultUserServer && alias.User != "" {
					name = alias.User
					break
				}
			}
		}
		if name == "" {
			continue
		}
		for _, alias := range aliases {
			previous := resolved[alias]
			if rank > previous.rank || (rank == previous.rank && (previous.name == "" || name < previous.name)) {
				resolved[alias] = rankedName{name: name, rank: rank}
			}
		}
	}
	// Authenticated self identity wins over a numeric address-book placeholder.
	selfName := selfDisplayNameWithContacts(ctx, bridgeClient, bridgeClient.Store.Contacts)
	selfRank := 4
	if selfName == "" {
		selfRank = 0
		for _, jid := range selfIdentityJIDs(ctx, bridgeClient) {
			if jid.Server == types.DefaultUserServer && jid.User != "" {
				selfName = jid.User
				break
			}
		}
	}
	if selfName != "" {
		for _, jid := range selfIdentityJIDs(ctx, bridgeClient) {
			if previous := resolved[jid]; previous.name == "" || selfRank > previous.rank {
				resolved[jid] = rankedName{name: selfName, rank: selfRank}
			}
		}
	}
	jids := make([]types.JID, 0, len(resolved))
	for jid := range resolved {
		jids = append(jids, jid)
	}
	sort.Slice(jids, func(i, j int) bool { return jids[i].String() < jids[j].String() })
	entries := make([]contactEntry, 0, len(jids))
	for _, jid := range jids {
		entries = append(entries, contactEntry{jid: jid, name: resolved[jid].name})
	}
	return entries, nil
}

//export C_GetContacts
func C_GetContacts() C.GetContactsResult {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil {
		return contactEntriesToC(nil)
	}
	ctx := context.Background()
	entries := lookupContactEntries(ctx, clientSnapshot)

	// Groups remain in this bridge wrapper; contacts.go owns only contact lookup.
	groups, err := clientSnapshot.GetJoinedGroups(ctx)
	if err != nil {
		panic(err)
	}
	for _, group := range groups {
		entries = append(entries, contactEntry{jid: group.JID, name: group.GroupName.Name})
	}
	return contactEntriesToC(entries)
}

func contactEntriesToC(entries []contactEntry) C.GetContactsResult {
	if len(entries) == 0 {
		return C.GetContactsResult{}
	}
	cEntries := C.malloc(C.size_t(len(entries)) * C.size_t(unsafe.Sizeof(C.ContactEntry{})))
	entryList := unsafe.Slice((*C.ContactEntry)(cEntries), len(entries))
	for i, entry := range entries {
		entryList[i] = C.ContactEntry{jid: jidToC(entry.jid), name: C.CString(entry.name)}
	}
	return C.GetContactsResult{entries: (*C.ContactEntry)(cEntries), size: C.uint32_t(len(entries))}
}

func freeContactResult(result C.GetContactsResult) {
	if result.entries == nil {
		return
	}
	entries := unsafe.Slice(result.entries, int(result.size))
	for _, entry := range entries {
		C.free(unsafe.Pointer(entry.jid))
		C.free(unsafe.Pointer(entry.name))
	}
	C.free(unsafe.Pointer(result.entries))
}

// C_FreeContacts releases the entries and strings returned by C_GetContacts.
// The caller owns the result and must invoke this exactly once after copying
// all entries. Empty results are valid and nil-safe.
//
//export C_FreeContacts
func C_FreeContacts(result C.GetContactsResult) {
	freeContactResult(result)
}

func contactEntryStrings(entry C.ContactEntry) (string, string) {
	return C.GoString(entry.jid), C.GoString(entry.name)
}

func contactJIDs(ctx context.Context, jid types.JID, lids store.LIDStore) []types.JID {
	jids := make([]types.JID, 0, 5)
	appendUnique := func(candidate types.JID) {
		if candidate.IsEmpty() {
			return
		}
		for _, existing := range jids {
			if existing == candidate {
				return
			}
		}
		jids = append(jids, candidate)
	}
	appendUnique(jid)
	appendUnique(jid.ToNonAD())
	if lids == nil {
		return jids
	}
	canonical := jid.ToNonAD()
	switch canonical.Server {
	case types.HiddenUserServer:
		if pn, err := lids.GetPNForLID(ctx, canonical); err == nil {
			appendUnique(pn)
			appendUnique(pn.ToNonAD())
		}
	case types.DefaultUserServer:
		if lid, err := lids.GetLIDForPN(ctx, canonical); err == nil {
			appendUnique(lid)
			appendUnique(lid.ToNonAD())
		}
	}
	return jids
}
