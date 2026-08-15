package main

import (
	"context"
	"errors"
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestFetchGroupInfoPreservesStatusesAndGroupFields(t *testing.T) {
	wantErr := errors.New("group request failed")
	group := types.NewJID("123", types.GroupServer)
	cases := []struct {
		name       string
		jid        types.JID
		lookup     groupInfoLookup
		wantStatus uint8
		wantResult groupInfoResult
	}{
		{name: "not a group", jid: types.NewJID("1", types.DefaultUserServer), lookup: func(context.Context, types.JID) (*types.GroupInfo, error) {
			t.Fatal("lookup must not run")
			return nil, nil
		}, wantStatus: groupInfoStatusNotGroup},
		{name: "client unavailable", jid: group, wantStatus: groupInfoStatusClientUnavailable},
		{name: "request failed", jid: group, lookup: func(context.Context, types.JID) (*types.GroupInfo, error) { return nil, wantErr }, wantStatus: groupInfoStatusRequestFailed},
		{name: "successful group", jid: group, lookup: func(context.Context, types.JID) (*types.GroupInfo, error) {
			return &types.GroupInfo{GroupAnnounce: types.GroupAnnounce{IsAnnounce: true}, Participants: []types.GroupParticipant{{IsAdmin: true}}}, nil
		}, wantResult: groupInfoResult{announce: true, admin: true}},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			result := fetchGroupInfoWith(context.Background(), testCase.jid, testCase.lookup, func(types.GroupParticipant) bool { return true })
			if result.status != testCase.wantStatus {
				t.Fatalf("status = %d, want %d", result.status, testCase.wantStatus)
			}
			if testCase.wantResult != (groupInfoResult{}) && result != testCase.wantResult {
				t.Fatalf("result = %#v, want %#v", result, testCase.wantResult)
			}
		})
	}
}

func TestFetchGroupInfoRequiresIdentityMatchForAdmin(t *testing.T) {
	group := types.NewJID("123", types.GroupServer)
	info := &types.GroupInfo{Participants: []types.GroupParticipant{{IsAdmin: true}}}
	result := fetchGroupInfoWith(context.Background(), group, func(context.Context, types.JID) (*types.GroupInfo, error) { return info, nil }, func(types.GroupParticipant) bool { return false })
	if result.admin {
		t.Fatal("unmatched administrator must not grant admin permission")
	}
}

func TestGroupInfoResultToCPreservesABIFields(t *testing.T) {
	result := groupInfoResultToC(groupInfoResult{status: groupInfoStatusRequestFailed, announce: true, admin: true})
	if result.status != 3 || !bool(result.is_announce) || !bool(result.is_admin) {
		t.Fatalf("C result = %#v, want status 3 with announce and admin", result)
	}
}
