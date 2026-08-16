package main

import (
	"sync/atomic"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

const (
	messageActionEdit uint8 = iota
	messageActionDelete
)

type messageActionEvent struct {
	actionID, chat, sender, targetMessageID, replacement string
	occurredAt                                           int64
	arrivalOrder                                         uint64
	kind                                                 uint8
}

func replacementText(msg *waE2E.Message) (string, bool) {
	if msg == nil {
		return "", false
	}
	if replacement := msg.GetConversation(); replacement != "" {
		return replacement, true
	}
	if replacement := msg.GetExtendedTextMessage().GetText(); replacement != "" {
		return replacement, true
	}
	if pm := msg.GetProtocolMessage(); pm != nil && pm.GetEditedMessage() != nil {
		return replacementText(pm.GetEditedMessage())
	}
	if em := msg.GetEditedMessage(); em != nil && em.GetMessage() != nil {
		return replacementText(em.GetMessage())
	}
	return "", false
}

// messageActionEventFromProbe converts a previously located protocol envelope
// into the bridge action model. Unsupported or incomplete payloads stay ordinary.
func messageActionEventFromProbe(info types.MessageInfo, probe messageActionProbe) (messageActionEvent, bool, string) {
	protocol := probe.protocol
	if protocol == nil {
		return messageActionEvent{}, false, "protocol_absent"
	}
	if info.ID == "" {
		return messageActionEvent{}, false, "missing_action_id"
	}
	if protocol.GetKey() == nil {
		return messageActionEvent{}, false, "missing_protocol_key"
	}
	if protocol.GetKey().GetID() == "" {
		return messageActionEvent{}, false, "missing_target_id"
	}

	action := messageActionEvent{
		actionID: info.ID, chat: info.Chat.String(), sender: info.Sender.String(),
		targetMessageID: protocol.GetKey().GetID(), occurredAt: info.Timestamp.Unix(),
		arrivalOrder: atomic.AddUint64(&messageActionArrivalOrder, 1),
	}
	if timestampMS := protocol.GetTimestampMS(); timestampMS > 0 {
		action.occurredAt = timestampMS / 1000
	}
	if participant := protocol.GetKey().GetParticipant(); participant != "" {
		if sender, err := types.ParseJID(participant); err == nil {
			action.sender = sender.String()
		}
	}

	switch protocol.GetType() {
	case waE2E.ProtocolMessage_MESSAGE_EDIT:
		replacement, ok := replacementText(protocol.GetEditedMessage())
		if !ok {
			return messageActionEvent{}, false, "missing_replacement"
		}
		action.kind, action.replacement = messageActionEdit, replacement
	case waE2E.ProtocolMessage_REVOKE:
		action.kind = messageActionDelete
	default:
		return messageActionEvent{}, false, "unsupported_protocol"
	}
	return action, true, ""
}

func messageActionEventFromMessage(info types.MessageInfo, msg *waE2E.Message) (messageActionEvent, bool) {
	action, ok, _ := messageActionEventFromProbe(info, messageActionProbeFromMessage(msg, "message"))
	return action, ok
}
