package main

/*
#include <stdbool.h>
#include <stdint.h>
#include "callback_log_registration.h"

typedef struct {
	bool found;
	int64_t muted_until;
	bool pinned;
	bool archived;
} ChatSettings;
*/
import "C"

import (
	"context"
)

func chatSettingsPayloadToC(payload chatSettingsPayload) C.ChatSettings {
	return C.ChatSettings{
		found:       C.bool(payload.found),
		muted_until: C.int64_t(payload.mutedUntil),
		pinned:      C.bool(payload.pinned),
		archived:    C.bool(payload.archived),
	}
}

//export C_GetChatSettings
func C_GetChatSettings(cjid C.JID) C.ChatSettings {
	clientSnapshot := lifecycleState.clientSnapshot()
	ctx := context.Background()
	jid := cToJid(cjid).ToNonAD()
	settings, err := lookupChatSettings(
		ctx,
		jid,
		clientSnapshot.Store.ChatSettings.GetChatSettings,
		clientSnapshot.Store.LIDs.GetLIDForPN,
		clientSnapshot.Store.LIDs.GetPNForLID,
	)
	if err != nil {
		LOG_WARN("failed to get chat settings for %s: %v", jid, err)
		return C.ChatSettings{}
	}
	return chatSettingsPayloadToC(chatSettingsPayloadFrom(settings))
}
