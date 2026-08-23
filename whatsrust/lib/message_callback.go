package main

/*
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include "callback_log_registration.h"

static uint8_t* activeForwardSource;
static size_t activeForwardSourceLen;

void setActiveForwardSource(uint8_t* source, size_t length) {
	activeForwardSource = source;
	activeForwardSourceLen = length;
}

void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data) {
	Message copy = *data;
	copy.forwardSource = activeForwardSource;
	copy.forwardSourceLen = activeForwardSourceLen;
	hdl.callback(&copy, isSync, hdl.user_data);
}

void callOptimisticTextSentHandler(OptimisticTextSentHandler hdl, uint64_t localSendID, const Message* data) {
	Message copy = *data;
	hdl.callback(localSendID, &copy, hdl.user_data);
}
*/
import "C"

import (
	"sync"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

var messageCallbackMu sync.Mutex

type messageCallbackMetadata struct {
	id           string
	chat         types.JID
	sender       types.JID
	pushName     string
	mentionsSelf bool
	timestamp    int64
	isFromMe     bool
	forwarding   forwardingState
}

func messageCallbackMetadataFrom(info types.MessageInfo, msg *waE2E.Message) messageCallbackMetadata {
	return messageCallbackMetadata{
		id:           info.ID,
		chat:         info.Chat,
		sender:       info.Sender,
		pushName:     info.PushName,
		mentionsSelf: messageMentionsSelf(msg),
		timestamp:    info.Timestamp.Unix(),
		isFromMe:     info.IsFromMe,
		forwarding:   forwardingStateFromMessage(msg),
	}
}

type messageCallback struct {
	info        C.MessageInfo
	forwardData unsafe.Pointer
}

func beginMessageCallback(info types.MessageInfo, msg *waE2E.Message, rawSource []byte) *messageCallback {
	messageCallbackMu.Lock()
	rememberAuthenticatedPushName(info)
	callback := &messageCallback{}
	if len(rawSource) > 0 {
		callback.forwardData = C.CBytes(rawSource)
	}
	C.setActiveForwardSource((*C.uint8_t)(callback.forwardData), C.size_t(len(rawSource)))
	metadata := messageCallbackMetadataFrom(info, msg)
	callback.info = C.MessageInfo{
		id:              C.CString(metadata.id),
		chat:            jidToC(metadata.chat),
		sender:          jidToC(metadata.sender),
		pushName:        C.CString(metadata.pushName),
		mentionsSelf:    C.bool(metadata.mentionsSelf),
		timestamp:       C.int64_t(metadata.timestamp),
		isFromMe:        C.bool(metadata.isFromMe),
		quoteID:         nil,
		readBy:          C.uint16_t(0),
		isForwarded:     C.bool(metadata.forwarding.isForwarded),
		forwardingScore: C.uint32_t(metadata.forwarding.score),
	}
	return callback
}

func messageMentionsSelf(msg *waE2E.Message) bool {
	if client == nil || client.Store == nil || msg == nil {
		return false
	}
	for _, contextInfo := range messageContextInfos(msg) {
		for _, mentioned := range contextInfo.GetMentionedJID() {
			jid, err := types.ParseJID(mentioned)
			if err == nil && participantMatchesSelf(client, types.GroupParticipant{JID: jid}) {
				return true
			}
		}
	}
	return false
}

func messageContextInfos(msg *waE2E.Message) []*waE2E.ContextInfo {
	infos := make([]*waE2E.ContextInfo, 0, 1)
	if msg.ExtendedTextMessage != nil {
		infos = append(infos, msg.ExtendedTextMessage.GetContextInfo())
	}
	if msg.ImageMessage != nil {
		infos = append(infos, msg.ImageMessage.GetContextInfo())
	}
	if msg.VideoMessage != nil {
		infos = append(infos, msg.VideoMessage.GetContextInfo())
	}
	if msg.AudioMessage != nil {
		infos = append(infos, msg.AudioMessage.GetContextInfo())
	}
	if msg.DocumentMessage != nil {
		infos = append(infos, msg.DocumentMessage.GetContextInfo())
	}
	if msg.StickerMessage != nil {
		infos = append(infos, msg.StickerMessage.GetContextInfo())
	}
	return infos
}

func messageCallbackPushName(callback *messageCallback) string {
	return C.GoString(callback.info.pushName)
}

func (callback *messageCallback) close() {
	C.setActiveForwardSource(nil, 0)
	if callback.forwardData != nil {
		C.free(callback.forwardData)
	}
	if callback.info.pushName != nil {
		C.free(unsafe.Pointer(callback.info.pushName))
	}
	messageCallbackMu.Unlock()
}
