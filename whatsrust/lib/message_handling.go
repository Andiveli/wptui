package main

/*
#include "callback_log_registration.h"
*/
import "C"

import (
	"context"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func HandleMessage(info types.MessageInfo, msg *waE2E.Message, isSync bool) {
	msg, viewOnceUnavailable := unavailableViewOnceMessage(msg)

	info = normalizeMessageInfo(info)

	if dispatchMessageEvent(info, msg) {
		return
	}
	rawSource := forwardingSourcePayload(info, msg, viewOnceUnavailable)
	callback := beginMessageCallback(info, msg, rawSource)
	defer callback.close()
	cinfo := callback.info

	if msg.Conversation != nil {
		emitTextMessage(cinfo, msg.GetConversation(), isSync)
	}
	if msg.ExtendedTextMessage != nil {
		ext_msg := msg.GetExtendedTextMessage()

		context_info := ext_msg.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			// LOG_ERROR("asdfasdf %s", co)
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		mentionedJIDs := context_info.GetMentionedJID()
		text := replaceMentionedNames(
			ext_msg.GetText(),
			mentionedJIDs,
			mentionEntriesForGroup(context.Background(), info.Chat, mentionedJIDs...),
		)
		emitTextMessage(cinfo, text, isSync)
	}
	if msg.ImageMessage != nil {
		if !emitImageMessage(cinfo, info.ID, msg.GetImageMessage(), isSync) {
			return
		}
	}
	if msg.VideoMessage != nil {
		if !emitVideoMessage(cinfo, info.ID, msg.GetVideoMessage(), isSync) {
			return
		}
	}
	if msg.AudioMessage != nil {
		if !emitAudioMessage(cinfo, info.ID, msg.GetAudioMessage(), isSync) {
			return
		}
	}
	if msg.DocumentMessage != nil {
		if !emitDocumentMessage(cinfo, info.ID, msg.GetDocumentMessage(), isSync) {
			return
		}
	}
	if msg.StickerMessage != nil {
		if !emitStickerMessage(cinfo, info.ID, msg.GetStickerMessage(), isSync) {
			return
		}
	}
}

// HandleOptimisticTextSent delivers the immediate canonical result through a
// request-scoped callback. It deliberately bypasses HandleMessage so the later
// server echo remains a normal generic callback deduped by its server ID.
func HandleOptimisticTextSent(localSendID uint64, info types.MessageInfo, msg *waE2E.Message) {
	info = normalizeMessageInfo(info)
	callback := beginMessageCallback(info, msg, nil)
	defer callback.close()
	if msg.ExtendedTextMessage != nil {
		ext := msg.GetExtendedTextMessage()
		contextInfo := ext.GetContextInfo()
		if id := contextInfo.GetStanzaID(); id != "" {
			callback.info.quoteID = C.CString(id)
		}
		emitOptimisticTextMessage(callback.info, ext.GetText(), localSendID)
		return
	}
	if msg.Conversation != nil {
		emitOptimisticTextMessage(callback.info, msg.GetConversation(), localSendID)
	}
}
