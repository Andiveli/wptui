package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"strings"
	"sync/atomic"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types/events"
)

type decryptSecretEncryptedMessageFunc func(context.Context, *events.Message) (*waE2E.Message, error)

func decryptSecretEncryptedMessage(ctx context.Context, evt *events.Message) (*waE2E.Message, error) {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	return clientSnapshot.DecryptSecretEncryptedMessage(ctx, evt)
}

func secretEditErrorClass(err error) string {
	if errors.Is(err, whatsmeow.ErrOriginalMessageSecretNotFound) {
		return "missing_original_secret"
	}
	if strings.Contains(err.Error(), "decode message protobuf") {
		return "protobuf_decode_failed"
	}
	return "decrypt_or_auth_failed"
}

func secretEditCensusResult(evt *events.Message, secret *waE2E.SecretEncryptedMessage, result, contentKind, errorClass string) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	targetID := "<missing>"
	keyPresent := secret.GetTargetMessageKey() != nil
	if secret.GetTargetMessageKey().GetID() != "" {
		targetID = messageActionIdentifier(secret.GetTargetMessageKey().GetID())
	}
	messageActionCensusAppend(fmt.Sprintf("event_type=events_message secret_edit_result=%s error_class=%s secret_enc_type=%s secret_enc_type_number=%d action_id=%s target_id=%s target_key_present=%t secret_payload_length=%d secret_iv_length=%d decrypted_content_kind=%s", result, errorClass, safeCensusName(secret.GetSecretEncType().String()), int(secret.GetSecretEncType()), messageActionIdentifier(evt.Info.ID), targetID, keyPresent, len(secret.GetEncPayload()), len(secret.GetEncIV()), contentKind))
}

// messageActionEventFromSecretEncryptedMessage handles the encrypted remote edit
// envelope before ordinary dispatch. The target key identifies the edited message;
// its participant is deliberately not used as the action sender.
func messageActionEventFromSecretEncryptedMessage(evt *events.Message, decrypt decryptSecretEncryptedMessageFunc) (messageActionEvent, bool) {
	if evt == nil {
		return messageActionEvent{}, false
	}
	probe := messageActionProbeFromMessage(evt.RawMessage, "raw")
	if probe.secretEncrypted == nil {
		probe = messageActionProbeFromMessage(evt.Message, "message")
	}
	secret := probe.secretEncrypted
	if secret == nil || secret.GetSecretEncType() != waE2E.SecretEncryptedMessage_MESSAGE_EDIT {
		return messageActionEvent{}, false
	}
	if evt.Info.ID == "" {
		secretEditCensusResult(evt, secret, "ignored", "none", "missing_action_id")
		return messageActionEvent{}, false
	}
	if secret.GetTargetMessageKey() == nil || secret.GetTargetMessageKey().GetID() == "" {
		secretEditCensusResult(evt, secret, "ignored", "none", "missing_target_key")
		return messageActionEvent{}, false
	}
	if decrypt == nil {
		secretEditCensusResult(evt, secret, "failed", "none", "decrypt_unavailable")
		return messageActionEvent{}, false
	}

	// whatsmeow decrypts evt.Message, while the envelope may be nested under a
	// wrapper. Keep the incoming event immutable and present the located envelope.
	decryptEvent := *evt
	decryptEvent.Message = &waE2E.Message{SecretEncryptedMessage: secret}
	decrypted, err := decrypt(context.Background(), &decryptEvent)
	if err != nil {
		secretEditCensusResult(evt, secret, "failed", "none", secretEditErrorClass(err))
		return messageActionEvent{}, false
	}
	replacement, ok := replacementText(decrypted)
	if !ok {
		secretEditCensusResult(evt, secret, "ignored", msgContentTypes(decrypted), "missing_replacement")
		return messageActionEvent{}, false
	}
	contentKind := "conversation"
	if decrypted.GetConversation() == "" {
		contentKind = "extended_text"
	}
	secretEditCensusResult(evt, secret, "success", contentKind, "none")
	return messageActionEvent{
		actionID:        evt.Info.ID,
		chat:            evt.Info.Chat.String(),
		sender:          evt.Info.Sender.String(),
		targetMessageID: secret.GetTargetMessageKey().GetID(),
		replacement:     replacement,
		occurredAt:      evt.Info.Timestamp.Unix(),
		arrivalOrder:    atomic.AddUint64(&messageActionArrivalOrder, 1),
		kind:            messageActionEdit,
	}, true
}
