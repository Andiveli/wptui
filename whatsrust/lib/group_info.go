package main

/*
#include <stdbool.h>
#include <stdint.h>
#include "callback_log_registration.h"

typedef struct {
	uint8_t status;
	bool is_announce;
	bool is_admin;
} GroupInfoResult;
*/
import "C"

import (
	"context"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

const (
	groupInfoStatusOK uint8 = iota
	groupInfoStatusNotGroup
	groupInfoStatusClientUnavailable
	groupInfoStatusRequestFailed
)

const groupInfoTimeout = 5 * time.Second

var groupInfoClientUnavailable = groupInfoResult{status: groupInfoStatusClientUnavailable}

type groupInfoLookup func(context.Context, types.JID) (*types.GroupInfo, error)
type groupInfoSelfMatcher func(types.GroupParticipant) bool

type groupInfoResult struct {
	status   uint8
	announce bool
	admin    bool
}

func fetchGroupInfo(client *whatsmeow.Client, jid types.JID, matchesSelf groupInfoSelfMatcher) groupInfoResult {
	if client == nil {
		return groupInfoClientUnavailable
	}
	ctx, cancel := context.WithTimeout(context.Background(), groupInfoTimeout)
	defer cancel()
	return fetchGroupInfoWith(ctx, jid, client.GetGroupInfo, matchesSelf)
}

func fetchGroupInfoWith(ctx context.Context, jid types.JID, lookup groupInfoLookup, matchesSelf groupInfoSelfMatcher) groupInfoResult {
	if jid.Server != types.GroupServer {
		return groupInfoResult{status: groupInfoStatusNotGroup}
	}
	if lookup == nil {
		return groupInfoClientUnavailable
	}

	info, err := lookup(ctx, jid)
	if err != nil || info == nil {
		LOG_WARN("failed to get group info for %s: %v", jid, err)
		return groupInfoResult{status: groupInfoStatusRequestFailed}
	}

	result := groupInfoResult{
		announce: info.IsAnnounce,
	}
	for _, participant := range info.Participants {
		if matchesSelf != nil && matchesSelf(participant) {
			result.admin = participant.IsAdmin || participant.IsSuperAdmin
			break
		}
	}
	return result
}

//export C_GetGroupInfo
func C_GetGroupInfo(cjid C.JID) C.GroupInfoResult {
	if cjid == nil {
		return groupInfoResultToC(groupInfoClientUnavailable)
	}
	return groupInfoResultToC(fetchGroupInfo(client, cToJid(cjid).ToNonAD(), func(participant types.GroupParticipant) bool {
		return participantMatchesSelf(client, participant)
	}))
}

func groupInfoResultToC(result groupInfoResult) C.GroupInfoResult {
	return C.GroupInfoResult{
		status:      C.uint8_t(result.status),
		is_announce: C.bool(result.announce),
		is_admin:    C.bool(result.admin),
	}
}
