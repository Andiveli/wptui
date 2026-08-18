package main

import (
	"context"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

// GetChatId returns the normalized chat id (conversation key): LID→PN,
// broadcast→per-sender, status as-is.
func GetChatId(client *whatsmeow.Client, chatJid *types.JID, senderJid *types.JID) string {
	if chatJid == nil {
		LOG_WARN("chatJid is nil")
		return ""
	}
	if chatJid.Server == types.BroadcastServer && chatJid.User == "status" {
		return StrFromJid(*chatJid)
	}
	if chatJid.Server == types.BroadcastServer && chatJid.User != "status" {
		if senderJid != nil {
			userID := GetUserId(client, nil, senderJid)
			if userID == GetSelfId(client) {
				return StrFromJid(*chatJid)
			}
			return userID
		}
	}
	if chatJid.Server == types.HiddenUserServer {
		ctx := context.Background()
		if pChatJid, _ := client.Store.LIDs.GetPNForLID(ctx, *chatJid); !pChatJid.IsEmpty() {
			return StrFromJid(pChatJid)
		}
	}
	return StrFromJid(*chatJid)
}

// GetUserId returns the normalized user/sender id: LID→PN when known; in
// groups use sender as-is (like nchat).
func GetUserId(client *whatsmeow.Client, chatJid *types.JID, userJid *types.JID) string {
	if userJid == nil {
		LOG_WARN("userJid is nil")
		return ""
	}
	if chatJid != nil && chatJid.Server == types.GroupServer {
		return StrFromJid(*userJid)
	}
	if userJid.Server == types.HiddenUserServer {
		ctx := context.Background()
		if pUserJid, _ := client.Store.LIDs.GetPNForLID(ctx, *userJid); !pUserJid.IsEmpty() {
			return StrFromJid(pUserJid)
		}
	}
	return StrFromJid(*userJid)
}

// StrFromJid converts a JID to its identity string without mapping it.
func StrFromJid(jid types.JID) string {
	return jid.User + "@" + jid.Server
}
