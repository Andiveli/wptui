package main

/*
#include <stdint.h>

typedef const char* JID;
*/
import "C"

import (
	"context"
	"fmt"
	"strings"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

// reactionRequest keeps the reaction target separate from the destination
// chat. This is required for reactions to status messages.
type reactionRequest struct {
	target, destination, sender types.JID
	id                          types.MessageID
	reaction                    string
}

func newReactionRequest(target, destination, sender types.JID, id types.MessageID, reaction string) (reactionRequest, error) {
	if target.IsEmpty() || destination.IsEmpty() || sender.IsEmpty() || id == "" || reaction == "" {
		return reactionRequest{}, fmt.Errorf("reaction requires target, destination, sender, message ID, and text")
	}
	if target.Server == types.NewsletterServer || destination.Server == types.NewsletterServer {
		return reactionRequest{}, fmt.Errorf("newsletter reactions are not ordinary message reactions")
	}
	return reactionRequest{target: target, destination: destination, sender: sender, id: id, reaction: reaction}, nil
}

func buildOrdinaryReaction(client *whatsmeow.Client, target, sender types.JID, id types.MessageID, reaction string) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	request, err := newReactionRequest(target, target, sender, id, reaction)
	if err != nil {
		return nil, err
	}
	return client.BuildReaction(request.target, request.sender, request.id, request.reaction), nil
}

func buildOrdinaryEdit(client *whatsmeow.Client, chat types.JID, id types.MessageID, replacement string) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	if chat.IsEmpty() || id == "" || strings.TrimSpace(replacement) == "" {
		return nil, fmt.Errorf("edit requires chat, message ID, and replacement text")
	}
	if chat.Server == types.NewsletterServer {
		return nil, fmt.Errorf("newsletter edits are not ordinary message edits")
	}
	return client.BuildEdit(chat, id, &waE2E.Message{Conversation: &replacement}), nil
}

func buildOrdinaryRevoke(client *whatsmeow.Client, chat, sender types.JID, id types.MessageID) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	if chat.IsEmpty() || sender.IsEmpty() || id == "" {
		return nil, fmt.Errorf("revoke requires chat, sender, and message ID")
	}
	if chat.Server == types.NewsletterServer {
		return nil, fmt.Errorf("newsletter revocations are not ordinary message revocations")
	}
	return client.BuildRevoke(chat, sender, id), nil
}

func parseActionJID(raw string) (types.JID, error) {
	jid, err := types.ParseJID(raw)
	if err != nil || jid.IsEmpty() {
		return types.JID{}, fmt.Errorf("invalid JID")
	}
	return jid, nil
}

//export C_ReactToMessage
func C_ReactToMessage(targetJID C.JID, destinationJID C.JID, senderJID C.JID, messageID *C.char, reaction *C.char) C.uint8_t {
	if targetJID == nil || destinationJID == nil || senderJID == nil || messageID == nil || reaction == nil {
		return 1
	}
	target, err := parseActionJID(C.GoString(targetJID))
	if err != nil {
		return 1
	}
	destination, err := parseActionJID(C.GoString(destinationJID))
	if err != nil {
		return 1
	}
	sender, err := parseActionJID(C.GoString(senderJID))
	if err != nil {
		return 1
	}
	request, err := newReactionRequest(target, destination, sender, types.MessageID(C.GoString(messageID)), C.GoString(reaction))
	if err != nil {
		LOG_WARN("reaction rejected: %v", err)
		return 1
	}
	message, err := buildOrdinaryReaction(client, request.target, request.sender, request.id, request.reaction)
	if err != nil {
		LOG_WARN("reaction rejected: %v", err)
		return 1
	}
	if _, err := client.SendMessage(context.Background(), request.destination, message); err != nil {
		LOG_WARN("reaction send failed: %v", err)
		return 1
	}
	HandleMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: request.destination, Sender: *client.Store.ID, IsFromMe: true}}, message, false)
	return 0
}

//export C_EditMessage
func C_EditMessage(chatJID C.JID, messageID *C.char, replacement *C.char) C.uint8_t {
	if chatJID == nil || messageID == nil || replacement == nil {
		return 1
	}
	chat, err := parseActionJID(C.GoString(chatJID))
	if err != nil {
		return 1
	}
	message, err := buildOrdinaryEdit(client, chat, types.MessageID(C.GoString(messageID)), C.GoString(replacement))
	if err != nil {
		LOG_WARN("edit rejected: %v", err)
		return 1
	}
	if _, err := client.SendMessage(context.Background(), chat, message); err != nil {
		LOG_WARN("edit send failed: %v", err)
		return 1
	}
	return 0
}

//export C_RevokeMessage
func C_RevokeMessage(chatJID C.JID, senderJID C.JID, messageID *C.char) C.uint8_t {
	if chatJID == nil || senderJID == nil || messageID == nil {
		return 1
	}
	chat, err := parseActionJID(C.GoString(chatJID))
	if err != nil {
		return 1
	}
	sender, err := parseActionJID(C.GoString(senderJID))
	if err != nil {
		return 1
	}
	id := types.MessageID(C.GoString(messageID))
	message, err := buildOrdinaryRevoke(client, chat, sender, id)
	if err != nil {
		LOG_WARN("revoke rejected: %v", err)
		return 1
	}
	if _, err := client.SendMessage(context.Background(), chat, message); err != nil {
		LOG_WARN("revoke send failed: %v", err)
		return 1
	}
	removeForwardSources(chat.String(), string(id))
	return 0
}
