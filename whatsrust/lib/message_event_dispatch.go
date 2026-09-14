package main

import (
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func normalizeMessageInfo(info types.MessageInfo) types.MessageInfo {
	return normalizeMessageInfoWithClient(lifecycleState.clientSnapshot(), info)
}

func normalizeMessageInfoWithClient(clientSnapshot *whatsmeow.Client, info types.MessageInfo) types.MessageInfo {
	// Some history and DeviceSent events carry the sender identity while leaving
	// IsFromMe unset. Resolve that source identity before canonicalization so the
	// Rust side receives verified ownership for PN, LID, and device aliases.
	if !info.IsFromMe && participantMatchesSelf(clientSnapshot, types.GroupParticipant{JID: info.Sender}) {
		info.IsFromMe = true
	}
	// Normalize chat and sender ids (LID→PN, broadcast→per-sender) so Rust sees canonical ids.
	if normalizedChat := GetChatId(clientSnapshot, &info.Chat, &info.Sender); normalizedChat != "" {
		if jid, err := types.ParseJID(normalizedChat); err == nil {
			info.Chat = jid
		}
	}
	if normalizedSender := GetUserId(clientSnapshot, &info.Chat, &info.Sender); normalizedSender != "" {
		if jid, err := types.ParseJID(normalizedSender); err == nil {
			info.Sender = jid
		}
	}
	return info
}

func dispatchMessageEvent(info types.MessageInfo, msg *waE2E.Message) bool {
	if line, ok := statusProtocolReactionDiagnostic(info, msg); ok {
		emitStatusProtocolDiagnostic(messageActionDiagnostic, line)
	}
	for _, line := range statusProtocolContextDiagnostics(info, msg) {
		emitStatusProtocolDiagnostic(messageActionDiagnostic, line)
	}
	if reaction, ok := reactionEventFromMessage(info, msg); ok {
		dispatchReactionEvent(reaction)
		return true
	}
	if action, ok := messageActionEventFromMessage(info, msg); ok {
		if action.kind == messageActionDelete {
			removeForwardSources(action.chat, action.targetMessageID)
		}
		dispatchMessageActionEvent(action)
		return true
	}
	return false
}
