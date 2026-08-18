package main

import (
	"context"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

// participantMatchesSelf resolves the participant identities that can refer to
// the current user, including phone-number and LID representations.
func participantMatchesSelf(client *whatsmeow.Client, participant types.GroupParticipant) bool {
	if client == nil || client.Store == nil || client.Store.ID == nil {
		return false
	}
	self := client.Store.ID.ToNonAD()
	for _, candidate := range []types.JID{participant.JID, participant.PhoneNumber, participant.LID} {
		if candidate.IsZero() {
			continue
		}
		if jidsMatchSelf(self, candidate) {
			return true
		}
		if candidate.Server == types.HiddenUserServer && self.Server == types.DefaultUserServer {
			if phone, err := client.Store.LIDs.GetPNForLID(context.Background(), candidate); err == nil && phone.ToNonAD() == self {
				return true
			}
		}
	}
	return false
}

func jidsMatchSelf(self, candidate types.JID) bool {
	return !candidate.IsZero() && candidate.ToNonAD() == self.ToNonAD()
}
