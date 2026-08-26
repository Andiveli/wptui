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

// contactDisplayName follows the same fallback order as Rust get_contact_name.
func contactDisplayName(c types.ContactInfo) string {
	for _, candidate := range []string{c.FullName, c.FirstName, c.PushName, c.BusinessName} {
		if name := plainContactName(candidate); name != "" {
			return name
		}
	}
	return ""
}

func plainContactName(name string) string {
	name = strings.TrimSpace(name)
	for _, prefix := range []string{"~ ", "+ "} {
		if strings.HasPrefix(name, prefix) {
			return strings.TrimSpace(strings.TrimPrefix(name, prefix))
		}
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
	entries, err := loadContactEntries(context.Background(), client)
	if err != nil {
		return nil
	}
	return entries
}

func loadContactEntries(ctx context.Context, bridgeClient *whatsmeow.Client) ([]contactEntry, error) {
	if bridgeClient == nil || bridgeClient.Store == nil || bridgeClient.Store.Contacts == nil {
		return nil, nil
	}
	contacts, err := bridgeClient.Store.Contacts.GetAllContacts(ctx)
	if err != nil {
		return nil, err
	}
	entries := make([]contactEntry, 0, len(contacts))
	for jid, contact := range contacts {
		name := contactDisplayName(contact)
		if name == "" {
			continue
		}
		for _, alias := range contactJIDs(ctx, jid, bridgeClient.Store.LIDs) {
			entries = append(entries, contactEntry{jid: alias, name: name})
		}
	}
	return entries, nil
}

//export C_GetContacts
func C_GetContacts() C.GetContactsResult {
	if client == nil || client.Store == nil {
		return contactEntriesToC(nil)
	}
	ctx := context.Background()
	entries := lookupContactEntries(ctx, client)

	// Groups remain in this bridge wrapper; contacts.go owns only contact lookup.
	groups, err := client.GetJoinedGroups(ctx)
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
