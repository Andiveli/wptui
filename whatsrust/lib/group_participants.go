package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;
typedef struct {
	JID jid;
	JID phone_number;
	const char* name;
} GroupParticipantEntry;
typedef struct {
	GroupParticipantEntry* entries;
	uint32_t size;
} GroupParticipantsResult;
*/
import "C"

import (
	"context"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

//export C_GetGroupParticipants
func C_GetGroupParticipants(cjid C.JID) C.GroupParticipantsResult {
	if client == nil || cjid == nil {
		return C.GroupParticipantsResult{}
	}
	info, err := client.GetGroupInfo(context.Background(), cToJid(cjid).ToNonAD())
	if err != nil || info == nil {
		return C.GroupParticipantsResult{}
	}
	entries := make([]groupParticipantEntry, 0, len(info.Participants))
	var contacts store.ContactStore
	var lids store.LIDStore
	if client.Store != nil {
		contacts = client.Store.Contacts
		lids = client.Store.LIDs
	}
	participants := deduplicateGroupParticipants(context.Background(), info.Participants, lids)
	participants = excludeSelfGroupParticipants(context.Background(), participants)
	for _, participant := range participants {
		name := groupParticipantName(context.Background(), participant, contacts)
		semantic := participant.JID
		for _, jid := range []types.JID{participant.JID, participant.PhoneNumber, participant.LID} {
			if jid.IsEmpty() {
				continue
			}
			if name != "" {
				if semantic.IsEmpty() {
					semantic = jid
				}
				phoneNumber := participant.PhoneNumber
				if phoneNumber.IsEmpty() {
					phoneNumber = semantic
				}
				entries = append(entries, groupParticipantEntry{jid: semantic, phoneNumber: phoneNumber, name: name})
				break
			}
		}
	}
	return groupParticipantsToC(entries)
}

func groupParticipantName(ctx context.Context, participant types.GroupParticipant, contacts store.ContactStore) string {
	var lids store.LIDStore
	if client != nil && client.Store != nil {
		lids = client.Store.LIDs
	}
	isSelf := participantMatchesSelf(client, participant)
	if isSelf {
		if name := selfDisplayNameWithContacts(ctx, client, contacts); name != "" {
			return name
		}
	}
	var pushName string
	if contacts != nil {
		for _, jid := range participantJIDs(ctx, participant, lids) {
			contact, err := contacts.GetContact(ctx, jid)
			if err != nil {
				continue
			}
			if isSelf {
				if name := nonNumericLocalContactName(contact); name != "" {
					return name
				}
			} else if name := locallySavedContactName(contact); name != "" {
				return name
			}
			if !isSelf && pushName == "" {
				pushName = plainContactName(contact.PushName)
			}
		}
	}
	if isSelf {
		return participantFallbackName(participant)
	}
	if name := plainContactName(participant.DisplayName); name != "" {
		return name
	}
	if pushName != "" {
		return pushName
	}
	return participantFallbackName(participant)
}

func currentUserPushName(client *whatsmeow.Client) string {
	return selfDisplayName(context.Background(), client)
}

func excludeSelfGroupParticipants(ctx context.Context, participants []types.GroupParticipant) []types.GroupParticipant {
	result := make([]types.GroupParticipant, 0, len(participants))
	for _, participant := range participants {
		if participantMatchesSelf(client, participant) {
			continue
		}
		result = append(result, participant)
	}
	return result
}

func participantFallbackName(participant types.GroupParticipant) string {
	for _, jid := range []types.JID{participant.PhoneNumber, participant.JID, participant.LID} {
		if !jid.IsEmpty() && jid.User != "" {
			return jid.User
		}
	}
	return ""
}

func deduplicateGroupParticipants(ctx context.Context, participants []types.GroupParticipant, lids store.LIDStore) []types.GroupParticipant {
	result := make([]types.GroupParticipant, 0, len(participants))
	for _, participant := range participants {
		merged := false
		for index := range result {
			if !participantIdentitiesOverlap(ctx, result[index], participant, lids) {
				continue
			}
			mergeGroupParticipant(&result[index], participant)
			merged = true
			break
		}
		if !merged {
			result = append(result, participant)
		}
	}
	return result
}

func participantIdentitiesOverlap(ctx context.Context, left, right types.GroupParticipant, lids store.LIDStore) bool {
	for _, leftJID := range participantJIDs(ctx, left, lids) {
		for _, rightJID := range participantJIDs(ctx, right, lids) {
			if leftJID == rightJID {
				return true
			}
		}
	}
	return false
}

func mergeGroupParticipant(target *types.GroupParticipant, source types.GroupParticipant) {
	if target.JID.IsEmpty() {
		target.JID = source.JID
	}
	if target.PhoneNumber.IsEmpty() {
		target.PhoneNumber = source.PhoneNumber
	}
	if target.LID.IsEmpty() {
		target.LID = source.LID
	}
	if target.DisplayName == "" {
		target.DisplayName = source.DisplayName
	}
}

func participantJIDs(ctx context.Context, participant types.GroupParticipant, lids store.LIDStore) []types.JID {
	jids := make([]types.JID, 0, 5)
	appendUnique := func(jid types.JID) {
		if jid.IsEmpty() {
			return
		}
		for _, existing := range jids {
			if existing == jid {
				return
			}
		}
		jids = append(jids, jid)
	}
	for _, jid := range []types.JID{participant.JID, participant.PhoneNumber, participant.LID} {
		appendUnique(jid)
		appendUnique(jid.ToNonAD())
		if lids == nil {
			continue
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
	}
	return jids
}

func locallySavedContactName(contact types.ContactInfo) string {
	for _, candidate := range []string{contact.FullName, contact.FirstName} {
		if name := plainContactName(candidate); name != "" {
			return name
		}
	}
	return ""
}

//export C_FreeGroupParticipants
func C_FreeGroupParticipants(result C.GroupParticipantsResult) {
	freeGroupParticipantsResult(result)
}

type groupParticipantEntry struct {
	jid, phoneNumber types.JID
	name             string
}

func groupParticipantsToC(entries []groupParticipantEntry) C.GroupParticipantsResult {
	if len(entries) == 0 {
		return C.GroupParticipantsResult{}
	}
	ptr := C.malloc(C.size_t(len(entries)) * C.size_t(unsafe.Sizeof(C.GroupParticipantEntry{})))
	list := unsafe.Slice((*C.GroupParticipantEntry)(ptr), len(entries))
	for index, entry := range entries {
		phone := entry.phoneNumber
		if phone.IsEmpty() {
			phone = entry.jid
		}
		list[index] = C.GroupParticipantEntry{jid: jidToC(entry.jid), phone_number: jidToC(phone), name: C.CString(entry.name)}
	}
	return C.GroupParticipantsResult{entries: (*C.GroupParticipantEntry)(ptr), size: C.uint32_t(len(entries))}
}

func freeGroupParticipantsResult(result C.GroupParticipantsResult) {
	if result.entries == nil {
		return
	}
	entries := unsafe.Slice(result.entries, int(result.size))
	for _, entry := range entries {
		C.free(unsafe.Pointer(entry.jid))
		C.free(unsafe.Pointer(entry.phone_number))
		C.free(unsafe.Pointer(entry.name))
	}
	C.free(unsafe.Pointer(result.entries))
}
