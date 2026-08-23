package main

import (
	"context"
	"strings"
	"sync"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

var authenticatedPushNames = struct {
	sync.RWMutex
	values map[string]string
}{values: make(map[string]string)}

func authenticatedIdentityKey(client *whatsmeow.Client) string {
	if client == nil || client.Store == nil {
		return ""
	}
	if client.Store.ID != nil && !client.Store.ID.IsEmpty() {
		return client.Store.ID.ToNonAD().String()
	}
	if !client.Store.LID.IsEmpty() {
		return client.Store.LID.ToNonAD().String()
	}
	return ""
}

func clearAuthenticatedPushNameCache() {
	authenticatedPushNames.Lock()
	authenticatedPushNames.values = make(map[string]string)
	authenticatedPushNames.Unlock()
}

func rememberAuthenticatedPushName(info types.MessageInfo) {
	if !info.IsFromMe || strings.TrimSpace(info.PushName) == "" || client == nil || client.Store == nil {
		return
	}
	if !participantMatchesSelf(client, types.GroupParticipant{JID: info.Sender}) {
		return
	}
	key := authenticatedIdentityKey(client)
	if key == "" {
		return
	}
	authenticatedPushNames.Lock()
	authenticatedPushNames.values[key] = plainContactName(info.PushName)
	authenticatedPushNames.Unlock()
}

func authenticatedPushName(client *whatsmeow.Client) string {
	key := authenticatedIdentityKey(client)
	if key == "" {
		return ""
	}
	authenticatedPushNames.RLock()
	name := authenticatedPushNames.values[key]
	authenticatedPushNames.RUnlock()
	return name
}

// participantMatchesSelf resolves the participant identities that can refer to
// the current user, including phone-number and LID representations.
func participantMatchesSelf(client *whatsmeow.Client, participant types.GroupParticipant) bool {
	if client == nil || client.Store == nil {
		return false
	}

	ctx := context.Background()
	selfJIDs := selfIdentityJIDs(ctx, client)
	if len(selfJIDs) == 0 {
		return false
	}

	for _, candidate := range participantJIDs(ctx, participant, client.Store.LIDs) {
		for _, self := range selfJIDs {
			if jidsMatchSelf(self, candidate) {
				return true
			}
		}
	}
	return false
}

// selfIdentityJIDs returns only identities rooted in the authenticated device
// or in a verified PN/LID mapping. It deliberately does not infer a LID from
// its numeric user part.
func selfIdentityJIDs(ctx context.Context, client *whatsmeow.Client) []types.JID {
	if client == nil || client.Store == nil {
		return nil
	}
	identities := make([]types.JID, 0, 6)
	appendUnique := func(candidate types.JID) {
		if candidate.IsEmpty() {
			return
		}
		for _, existing := range identities {
			if existing == candidate {
				return
			}
		}
		identities = append(identities, candidate)
	}
	appendAliases := func(participant types.GroupParticipant) {
		for _, jid := range participantJIDs(ctx, participant, client.Store.LIDs) {
			appendUnique(jid)
		}
	}
	if client.Store.ID != nil && !client.Store.ID.IsEmpty() {
		appendAliases(types.GroupParticipant{JID: *client.Store.ID})
	}
	if !client.Store.LID.IsEmpty() {
		appendAliases(types.GroupParticipant{JID: client.Store.LID})
	}
	return identities
}

// selfDisplayName is the single fallback chain for the connected user's
// incoming mentions. Saved local names win over WhatsApp's account PushName.
func selfDisplayName(ctx context.Context, client *whatsmeow.Client) string {
	if client == nil || client.Store == nil {
		return ""
	}
	return selfDisplayNameWithContacts(ctx, client, client.Store.Contacts)
}

func selfDisplayNameWithContacts(ctx context.Context, client *whatsmeow.Client, contacts store.ContactStore) string {
	if client == nil || client.Store == nil {
		return ""
	}
	if contacts != nil {
		for _, jid := range selfIdentityJIDs(ctx, client) {
			contact, err := contacts.GetContact(ctx, jid)
			if err != nil {
				continue
			}
			if name := nonNumericLocalContactName(contact); name != "" {
				return name
			}
		}
	}
	if name := authenticatedPushName(client); name != "" && !phoneLikeName(name) {
		return name
	}
	if name := plainContactName(client.Store.PushName); name != "" && !phoneLikeName(name) {
		return name
	}
	return ""
}

func nonNumericLocalContactName(contact types.ContactInfo) string {
	for _, candidate := range []string{contact.FullName, contact.FirstName} {
		if name := plainContactName(candidate); name != "" && !phoneLikeName(name) {
			return name
		}
	}
	return ""
}

func phoneLikeName(name string) bool {
	digits := 0
	for _, r := range strings.TrimSpace(name) {
		switch {
		case r >= '0' && r <= '9':
			digits++
		case strings.ContainsRune(" +-().", r):
		default:
			return false
		}
	}
	return digits > 0
}

func selfMentionEntries(ctx context.Context, client *whatsmeow.Client) []contactEntry {
	name := selfDisplayName(ctx, client)
	if name == "" {
		return nil
	}
	entries := make([]contactEntry, 0, len(selfIdentityJIDs(ctx, client)))
	for _, jid := range selfIdentityJIDs(ctx, client) {
		entries = append(entries, contactEntry{jid: jid, name: name})
	}
	return entries
}

func jidsMatchSelf(self, candidate types.JID) bool {
	return !candidate.IsZero() && candidate.ToNonAD() == self.ToNonAD()
}
