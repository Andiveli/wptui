package main

/*
#include <stdint.h>

typedef const char* JID;

typedef struct {
	char* text;
} SendTextMessage;

typedef struct {
	uint8_t kind;
	char* path;
	char* fileID;
	char* caption;
} SendFileMessage;
*/
import "C"

import (
	"context"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func quotedContextInfo(id, sender, chat string) *waE2E.ContextInfo {
	return &waE2E.ContextInfo{StanzaID: &id, Participant: &sender, RemoteJID: &chat}
}

func quotedTextMessage(text string) *waE2E.Message {
	return &waE2E.Message{Conversation: &text}
}

func quotedFileMessage(kind uint8, caption string) *waE2E.Message {
	var captionPtr *string
	if caption != "" {
		captionPtr = &caption
	}
	switch kind {
	case FileTypeImage:
		return &waE2E.Message{ImageMessage: &waE2E.ImageMessage{Caption: captionPtr}}
	case FileTypeVideo:
		return &waE2E.Message{VideoMessage: &waE2E.VideoMessage{Caption: captionPtr}}
	case FileTypeAudio:
		return &waE2E.Message{AudioMessage: &waE2E.AudioMessage{}}
	case FileTypeDocument:
		return &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{Caption: captionPtr}}
	case FileTypeSticker:
		return &waE2E.Message{StickerMessage: &waE2E.StickerMessage{}}
	default:
		return nil
	}
}

func quotedMessageFromContent(messageType C.uint8_t, messageContent unsafe.Pointer) *waE2E.Message {
	if messageContent == nil {
		return nil
	}
	switch messageType {
	case C.uint8_t(MessageTypeText):
		return quotedTextMessage(C.GoString((*C.SendTextMessage)(messageContent).text))
	case C.uint8_t(MessageTypeFile):
		file := (*C.SendFileMessage)(messageContent)
		caption := ""
		if file.caption != nil {
			caption = C.GoString(file.caption)
		}
		return quotedFileMessage(uint8(file.kind), caption)
	default:
		return nil
	}
}

//export C_SendMessage
func C_SendMessage(cjid C.JID, messageType C.uint8_t, messageContent unsafe.Pointer, quoteId *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer) {
	if cjid == nil || messageContent == nil || client == nil || client.Store == nil || client.Store.ID == nil {
		LOG_WARN("message send rejected: client or message is unavailable")
		return
	}
	jid := cToJid(cjid)

	contextInfo := &waE2E.ContextInfo{}
	if quoteId != nil {
		contextInfo = quotedContextInfo(C.GoString(quoteId), C.GoString(quoteSender), C.GoString(quoteChat))
		contextInfo.QuotedMessage = quotedMessageFromContent(quoteMessageType, quoteMessageContent)
	}

	message := ContentToWaE2EMessage(messageType, messageContent, contextInfo)

	sendResponse, err := client.SendMessage(context.Background(), jid, message)
	if err != nil {
		LOG_WARN("message send failed: %v", err)
		return
	}

	messageInfo := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: jid, Sender: *client.Store.ID, IsFromMe: true},
		ID:            sendResponse.ID,
		Timestamp:     sendResponse.Timestamp,
	}
	LOG_INFO("Message sent: %s %s", messageInfo.ID, messageInfo.Chat)
	HandleMessage(messageInfo, message, false)
}
