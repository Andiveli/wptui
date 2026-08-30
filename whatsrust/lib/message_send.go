package main

/*
#include <stdint.h>

typedef const char* JID;

typedef struct {
	char* text;
	JID* mentionedJIDs;
	uintptr_t mentionedCount;
} SendTextMessage;

typedef struct {
	uint8_t kind;
	char* path;
	char* fileID;
	char* caption;
	JID* mentionedJIDs;
	uintptr_t mentionedCount;
} SendFileMessage;

*/
import "C"

import (
	"fmt"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

// ContentToWaE2EMessage converts the public FFI payload into a WhatsApp
// message. File construction remains delegated to the injectable builder.
func ContentToWaE2EMessage(messageType C.uint8_t, messageContent unsafe.Pointer, contextInfo *waE2E.ContextInfo) *waE2E.Message {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil {
		panic("WhatsApp client is unavailable")
	}
	switch messageType {
	case C.uint8_t(MessageTypeText):
		textMsg := (*C.SendTextMessage)(messageContent)
		return contentToWaE2EMessage(
			MessageTypeText,
			C.GoString(textMsg.text),
			mentionedJIDs(textMsg.mentionedJIDs, textMsg.mentionedCount),
			0,
			"",
			nil,
			contextInfo,
			clientSnapshot.Upload,
		)
	case C.uint8_t(MessageTypeFile):
		fileMsg := (*C.SendFileMessage)(messageContent)
		kind := uint8(fileMsg.kind)
		filePath := C.GoString(fileMsg.path)
		var caption *string
		if fileMsg.caption != nil {
			captionValue := C.GoString(fileMsg.caption)
			caption = &captionValue
		}
		return contentToWaE2EMessage(
			MessageTypeFile,
			"",
			mentionedJIDs(fileMsg.mentionedJIDs, fileMsg.mentionedCount),
			kind,
			filePath,
			caption,
			contextInfo,
			clientSnapshot.Upload,
		)
	default:
		panic(fmt.Sprintf("Unsupported message type: %d", messageType))
	}
}

func setMentionedJIDs(contextInfo *waE2E.ContextInfo, ptr *C.JID, count C.uintptr_t) {
	if contextInfo != nil {
		setMentionedJIDsFromStrings(contextInfo, mentionedJIDs(ptr, count))
	}
}

func mentionedJIDs(ptr *C.JID, count C.uintptr_t) []string {
	if ptr == nil || count == 0 {
		return nil
	}
	result := make([]string, 0, int(count))
	for _, jid := range unsafe.Slice(ptr, int(count)) {
		if jid == nil {
			continue
		}
		parsed, err := types.ParseJID(C.GoString(jid))
		if err == nil && !parsed.IsEmpty() {
			result = append(result, parsed.String())
		}
	}
	return normalizeMentionedJIDs(result)
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
	status := C_SendOutboundMessage(cjid, messageType, messageContent, quoteId, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, 0)
	if status != C.uint8_t(outboundSendSent) {
		LOG_WARN("message send failed with status %d", status)
	}
}

//export C_SendTextMessage
func C_SendTextMessage(cjid C.JID, messageContent unsafe.Pointer, quoteId *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID C.uint64_t) C.uint8_t {
	return C_SendOutboundMessage(cjid, C.uint8_t(MessageTypeText), messageContent, quoteId, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, localSendID)
}

//export C_SendOutboundMessage
func C_SendOutboundMessage(cjid C.JID, messageType C.uint8_t, messageContent unsafe.Pointer, quoteID *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID C.uint64_t) C.uint8_t {
	request, ok := textSendRequestFromC(cjid, messageType, messageContent, quoteID, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, uint64(localSendID))
	if !ok {
		return C.uint8_t(outboundSendInvalidRequest)
	}
	return C.uint8_t(sendOutboundRequest(request))
}

func textSendRequestFromC(cjid C.JID, messageType C.uint8_t, messageContent unsafe.Pointer, quoteID *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID uint64) (textSendRequest, bool) {
	if cjid == nil || messageContent == nil {
		return textSendRequest{}, false
	}
	request := textSendRequest{messageType: uint8(messageType), chat: cToJid(cjid), localSendID: localSendID}
	switch messageType {
	case C.uint8_t(MessageTypeText):
		textMessage := (*C.SendTextMessage)(messageContent)
		request.text = C.GoString(textMessage.text)
		request.mentionedJIDs = mentionedJIDs(textMessage.mentionedJIDs, textMessage.mentionedCount)
	case C.uint8_t(MessageTypeFile):
		fileMessage := (*C.SendFileMessage)(messageContent)
		request.fileKind = uint8(fileMessage.kind)
		request.filePath = C.GoString(fileMessage.path)
		request.mentionedJIDs = mentionedJIDs(fileMessage.mentionedJIDs, fileMessage.mentionedCount)
		if fileMessage.caption != nil {
			caption := C.GoString(fileMessage.caption)
			request.caption = &caption
		}
	default:
		return textSendRequest{}, false
	}
	if quoteID != nil {
		request.quote = &textSendQuote{
			stanzaID: C.GoString(quoteID), participant: C.GoString(quoteSender), remoteJID: C.GoString(quoteChat),
			content: quotedMessageFromContent(quoteMessageType, quoteMessageContent),
		}
	}
	return request, true
}

func sendNormalTextRequest(request textSendRequest) uint8 {
	request.localSendID = 0
	return sendOutboundRequest(request)
}

func sendOptimisticTextRequest(request textSendRequest) uint8 {
	return sendOutboundRequest(request)
}
