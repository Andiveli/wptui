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
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

type contactEntry struct {
	jid  types.JID
	name string
}

// contactDisplayName follows the same fallback order as Rust get_contact_name.
func contactDisplayName(c types.ContactInfo) string {
	switch {
	case c.FullName != "":
		return c.FullName
	case c.FirstName != "":
		return c.FirstName
	case c.PushName != "":
		return "~ " + c.PushName
	case c.BusinessName != "":
		return "+ " + c.BusinessName
	default:
		return ""
	}
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
		entries = append(entries, contactEntry{jid: jid, name: name})
		if jid.Server != types.HiddenUserServer && bridgeClient.Store.LIDs != nil {
			if lid, _ := bridgeClient.Store.LIDs.GetLIDForPN(ctx, jid); !lid.IsEmpty() {
				entries = append(entries, contactEntry{jid: lid, name: name})
			}
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

func contactEntryStrings(entry C.ContactEntry) (string, string) {
	return C.GoString(entry.jid), C.GoString(entry.name)
}
