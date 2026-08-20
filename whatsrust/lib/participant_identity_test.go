package main

import (
	"context"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

func TestJidsMatchSelfNormalizesDeviceIdentity(t *testing.T) {
	self := types.NewADJID("self", 0, 7)
	candidate := types.NewJID("self", types.DefaultUserServer)
	if !jidsMatchSelf(self, candidate) {
		t.Fatal("device and user JIDs should identify the same participant")
	}
	if jidsMatchSelf(self, types.NewJID("other", types.DefaultUserServer)) {
		t.Fatal("different participants must not match")
	}
}

func TestParticipantMatchesSelfUsesPNLIDAliasesWithoutCrossServerGuessing(t *testing.T) {
	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	client := &whatsmeow.Client{Store: &store.Device{
		ID:  &pn,
		LID: lid,
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}

	for name, participant := range map[string]types.GroupParticipant{
		"self PN":    {JID: pn},
		"self AD PN": {JID: types.NewADJID("123", 0, 7)},
		"self LID":   {JID: lid},
		"mapped LID": {JID: types.NewJID("456", types.HiddenUserServer)},
		"mapped PN":  {JID: types.NewJID("123", types.DefaultUserServer)},
	} {
		t.Run(name, func(t *testing.T) {
			if !participantMatchesSelf(client, participant) {
				t.Fatalf("participantMatchesSelf(%#v) = false, want true", participant)
			}
		})
	}

	if participantMatchesSelf(client, types.GroupParticipant{JID: types.NewJID("123", "newsletter")}) {
		t.Fatal("same numeric user on another server must not match")
	}
}

func TestParticipantMatchesSelfHandlesNilLIDStore(t *testing.T) {
	pn := types.NewJID("123", types.DefaultUserServer)
	client := &whatsmeow.Client{Store: &store.Device{ID: &pn}}
	if participantMatchesSelf(client, types.GroupParticipant{JID: types.NewJID("456", types.HiddenUserServer)}) {
		t.Fatal("unmapped LID must not match with a nil LID store")
	}
}

type participantIdentityLIDStore struct {
	pnByLID map[types.JID]types.JID
	lidByPN map[types.JID]types.JID
}

func (s participantIdentityLIDStore) PutManyLIDMappings(context.Context, []store.LIDMapping) error {
	return nil
}

func (s participantIdentityLIDStore) PutLIDMapping(context.Context, types.JID, types.JID) error {
	return nil
}

func (s participantIdentityLIDStore) GetPNForLID(_ context.Context, lid types.JID) (types.JID, error) {
	return s.pnByLID[lid], nil
}

func (s participantIdentityLIDStore) GetLIDForPN(_ context.Context, pn types.JID) (types.JID, error) {
	return s.lidByPN[pn], nil
}

func (participantIdentityLIDStore) GetManyLIDsForPNs(context.Context, []types.JID) (map[types.JID]types.JID, error) {
	return nil, nil
}
