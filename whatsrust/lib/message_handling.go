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

func HandleViewOnceUnavailableMessage(info types.MessageInfo, isSync bool) {
	info = normalizeMessageInfo(info)
	callback := beginMessageCallback(info, nil, nil)
	defer callback.close()
	emitViewOnceUnavailableMessage(callback.info, isSync)
}

func HandleMessage(info types.MessageInfo, msg *waE2E.Message, isSync bool) {
	info = normalizeMessageInfo(info)

	if dispatchMessageEvent(info, msg) {
		return
	}
	rawSource := forwardingSourcePayload(info, msg, false)
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
				callback.setQuoteID(id)
				cinfo.quoteID = callback.info.quoteID
			}
		}

		text := replaceMessageMentionNames(info.Chat, ext_msg.GetText(), context_info)
		emitTextMessage(cinfo, text, isSync)
	}
	if msg.ImageMessage != nil {
		if !emitImageMessage(callback, info.ID, msg.GetImageMessage(), isSync) {
			return
		}
	}
	if msg.VideoMessage != nil {
		if !emitVideoMessage(callback, info.ID, msg.GetVideoMessage(), isSync) {
			return
		}
	}
	if msg.AudioMessage != nil {
		if !emitAudioMessage(callback, info.ID, msg.GetAudioMessage(), isSync) {
			return
		}
	}
	if msg.DocumentMessage != nil {
		if !emitDocumentMessage(callback, info.ID, msg.GetDocumentMessage(), isSync) {
			return
		}
	}
	if msg.StickerMessage != nil {
		if !emitStickerMessage(callback, info.ID, msg.GetStickerMessage(), isSync) {
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
		if contextInfo != nil {
			if id := contextInfo.GetStanzaID(); id != "" {
				callback.info.quoteID = C.CString(id)
			}
		}
		text := replaceMessageMentionNames(info.Chat, ext.GetText(), contextInfo)
		emitOptimisticTextMessage(callback.info, text, localSendID)
		return
	}
	if msg.Conversation != nil {
		emitOptimisticTextMessage(callback.info, msg.GetConversation(), localSendID)
		return
	}
	if image := msg.GetImageMessage(); image != nil {
		emitOptimisticFileMessage(callback.info, FileTypeImage, "", "", image.GetCaption(), localSendID)
		return
	}
	if video := msg.GetVideoMessage(); video != nil {
		emitOptimisticFileMessage(callback.info, FileTypeVideo, "", "", video.GetCaption(), localSendID)
		return
	}
	if audio := msg.GetAudioMessage(); audio != nil {
		emitOptimisticFileMessage(callback.info, FileTypeAudio, "", "", "", localSendID)
		return
	}
	if document := msg.GetDocumentMessage(); document != nil {
		emitOptimisticFileMessage(callback.info, FileTypeDocument, "", "", document.GetCaption(), localSendID)
		return
	}
	if msg.GetStickerMessage() != nil {
		emitOptimisticFileMessage(callback.info, FileTypeSticker, "", "", "", localSendID)
	}
}

func replaceMessageMentionNames(chat types.JID, text string, contextInfo *waE2E.ContextInfo) string {
	if contextInfo == nil {
		return text
	}
	mentionedJIDs := contextInfo.GetMentionedJID()
	return replaceMentionedNames(
		text,
		mentionedJIDs,
		mentionEntriesForGroup(context.Background(), chat, mentionedJIDs...),
	)
}
