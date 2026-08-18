package main

import (
	"fmt"
	"os"
	"strings"
	"sync"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

const messageActionCensusLimit = 100

type messageActionCensus struct {
	mu      sync.Mutex
	nextSeq uint64
	entries []string
}

var eventCensus messageActionCensus

func messageActionCensusAppend(entry string) {
	eventCensus.mu.Lock()
	eventCensus.nextSeq++
	entry = fmt.Sprintf("census=event seq=%d %s", eventCensus.nextSeq, entry)
	if len(eventCensus.entries) == messageActionCensusLimit {
		eventCensus.entries = eventCensus.entries[1:]
	}
	eventCensus.entries = append(eventCensus.entries, entry)
	eventCensus.mu.Unlock()
	messageActionDiagnostic("%s", entry)
}

func messageActionCensusDiagnostic(rawEvt any) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	messageActionCensusAppend(messageActionCensusLine(rawEvt))
}

func messageActionCensusLine(rawEvt any) string {
	eventType := strings.NewReplacer("*", "", ".", "_").Replace(fmt.Sprintf("%T", rawEvt))
	eventType = strings.ToLower(eventType)
	switch evt := rawEvt.(type) {
	case *events.Message:
		return messageActionMessageCensusLine(eventType, evt)
	case *events.AppState:
		return fmt.Sprintf("event_type=%s subtype=%s", eventType, appStateCensusSubtype(evt))
	case *events.AppStateSyncComplete:
		return fmt.Sprintf("event_type=%s subtype=sync_complete app_state=%s", eventType, safeCensusName(string(evt.Name)))
	case *events.AppStateSyncError:
		return fmt.Sprintf("event_type=%s subtype=sync_error app_state=%s", eventType, safeCensusName(string(evt.Name)))
	case *events.Receipt:
		return fmt.Sprintf("event_type=%s subtype=receipt_%s", eventType, receiptCensusSubtype(evt.Type))
	case *events.UndecryptableMessage:
		return fmt.Sprintf("event_type=%s subtype=undecryptable_%s", eventType, safeCensusName(string(evt.UnavailableType)))
	default:
		return fmt.Sprintf("event_type=%s", eventType)
	}
}

func receiptCensusSubtype(receiptType types.ReceiptType) string {
	switch receiptType {
	case types.ReceiptTypeDelivered:
		return "delivered"
	case types.ReceiptTypeSender:
		return "sender"
	case types.ReceiptTypeRetry:
		return "retry"
	case types.ReceiptTypeRead:
		return "read"
	case types.ReceiptTypeReadSelf:
		return "read_self"
	case types.ReceiptTypePlayed:
		return "played"
	default:
		return "other"
	}
}

func messageActionMessageCensusLine(eventType string, evt *events.Message) string {
	raw := messageActionProbeFromMessage(evt.RawMessage, "raw")
	normalized := messageActionProbeFromMessage(evt.Message, "message")
	source := messageActionProbe{}
	var sourceMessage *waE2E.Message
	if evt.SourceWebMsg != nil {
		sourceMessage = evt.SourceWebMsg.GetMessage()
		source = messageActionProbeFromMessage(sourceMessage, "source")
	}
	selected := raw
	if selected.protocol == nil {
		selected = normalized
	}
	if selected.protocol == nil {
		selected = source
	}
	protocolType := "none"
	protocolKeyID := false
	if selected.protocol != nil {
		protocolType = safeCensusName(selected.protocol.GetType().String())
		protocolKeyID = selected.protocol.GetKey().GetID() != ""
	}
	secret := raw.secretEncrypted
	if secret == nil {
		secret = normalized.secretEncrypted
	}
	if secret == nil {
		secret = source.secretEncrypted
	}
	secretType := "none"
	secretTypeNumber := 0
	secretTargetPresent := false
	secretTargetID := "<missing>"
	secretPayloadLength := 0
	secretIVLength := 0
	if secret != nil {
		secretType = safeCensusName(secret.GetSecretEncType().String())
		secretTypeNumber = int(secret.GetSecretEncType())
		secretTargetPresent = secret.GetTargetMessageKey() != nil
		if secret.GetTargetMessageKey().GetID() != "" {
			secretTargetID = messageActionIdentifier(secret.GetTargetMessageKey().GetID())
		}
		secretPayloadLength = len(secret.GetEncPayload())
		secretIVLength = len(secret.GetEncIV())
	}
	sourceKey := "<missing>"
	if evt.SourceWebMsg != nil && evt.SourceWebMsg.GetKey().GetID() != "" {
		sourceKey = messageActionIdentifier(evt.SourceWebMsg.GetKey().GetID())
	}
	// whatsmeow has no IsHistory flag; SourceWebMsg marks parsed history or an unavailable-message response.
	return fmt.Sprintf("event_type=%s is_edit=%t is_history=%t info_id=%s chat=%s sender=%s roots=raw:%t,message:%t,source_web_msg:%t raw_kinds=%s message_kinds=%s source_kinds=%s wrappers=%s protocol_present=%t protocol_type=%s protocol_key_id=%t source_key=%s secret_enc_type=%s secret_enc_type_number=%d secret_target_present=%t secret_target_id=%s secret_payload_length=%d secret_iv_length=%d decrypt_result=not_attempted decrypted_content_kind=none", eventType, evt.IsEdit, evt.SourceWebMsg != nil, messageActionIdentifier(evt.Info.ID), messageActionIdentifier(evt.Info.Chat.String()), messageActionIdentifier(evt.Info.Sender.String()), evt.RawMessage != nil, evt.Message != nil, evt.SourceWebMsg != nil, messageCensusKinds(evt.RawMessage), messageCensusKinds(evt.Message), messageCensusKinds(sourceMessage), messageCensusWrappers(evt.RawMessage, "raw"), selected.protocol != nil, protocolType, protocolKeyID, sourceKey, secretType, secretTypeNumber, secretTargetPresent, secretTargetID, secretPayloadLength, secretIVLength)
}

func appStateCensusSubtype(evt *events.AppState) string {
	switch {
	case evt.GetDeleteMessageForMeAction() != nil:
		return "delete_message_for_me"
	case evt.GetStarAction() != nil:
		return "star"
	case evt.GetLabelAssociationAction() != nil:
		return "label_association"
	default:
		return "other"
	}
}

func safeCensusName(value string) string {
	value = strings.ToLower(value)
	value = strings.NewReplacer(".", "_", "-", "_", " ", "_").Replace(value)
	return strings.Map(func(character rune) rune {
		if character >= 'a' && character <= 'z' || character >= '0' && character <= '9' || character == '_' {
			return character
		}
		return -1
	}, value)
}

func messageActionIdentifier(identifier string) string {
	if identifier == "" {
		return "<missing>"
	}
	hash := uint64(0xcbf29ce484222325)
	for _, byte := range []byte(identifier) {
		hash ^= uint64(byte)
		hash *= 0x100000001b3
	}
	return fmt.Sprintf("<id:%08x>", hash)
}
