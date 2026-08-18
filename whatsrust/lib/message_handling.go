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
		extMsg := msg.GetExtendedTextMessage()

		contextInfo := extMsg.GetContextInfo()
		if contextInfo != nil {
			id := contextInfo.GetStanzaID()
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		emitTextMessage(cinfo, extMsg.GetText(), isSync)
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
