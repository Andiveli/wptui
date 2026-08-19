package main

/*
#include "callback_log_registration.h"
*/
import "C"

import (
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

		emitTextMessage(cinfo, textWithMentionNames(ext_msg.GetText(), context_info), isSync)
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
