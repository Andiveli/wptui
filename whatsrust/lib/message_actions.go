package main

/*
#include <stdint.h>

typedef const char* JID;
*/
import "C"

import (
	"context"

	"go.mau.fi/whatsmeow/types"
)

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
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil || clientSnapshot.Store.ID == nil {
		return 1
	}
	message, err := buildOrdinaryReaction(clientSnapshot, request.target, request.sender, request.id, request.reaction)
	if err != nil {
		LOG_WARN("reaction rejected: %v", err)
		return 1
	}
	if _, err := clientSnapshot.SendMessage(context.Background(), request.destination, message); err != nil {
		LOG_WARN("reaction send failed: %v", err)
		return 1
	}
	HandleMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: request.destination, Sender: *clientSnapshot.Store.ID, IsFromMe: true}}, message, false)
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
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil {
		return 1
	}
	message, err := buildOrdinaryEdit(clientSnapshot, chat, types.MessageID(C.GoString(messageID)), C.GoString(replacement))
	if err != nil {
		LOG_WARN("edit rejected: %v", err)
		return 1
	}
	if _, err := clientSnapshot.SendMessage(context.Background(), chat, message); err != nil {
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
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil {
		return 1
	}
	message, err := buildOrdinaryRevoke(clientSnapshot, chat, sender, id)
	if err != nil {
		LOG_WARN("revoke rejected: %v", err)
		return 1
	}
	if _, err := clientSnapshot.SendMessage(context.Background(), chat, message); err != nil {
		LOG_WARN("revoke send failed: %v", err)
		return 1
	}
	removeForwardSources(chat.String(), string(id))
	return 0
}
