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
	"context"
	"fmt"
	"time"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

const optimisticTextSendTimeout = 5 * time.Second

func optimisticTextSendContext(parent context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	return context.WithTimeout(parent, timeout)
}

// ContentToWaE2EMessage converts the public FFI payload into a WhatsApp
// message. File construction remains delegated to the injectable builder.
func ContentToWaE2EMessage(messageType C.uint8_t, messageContent unsafe.Pointer, contextInfo *waE2E.ContextInfo) *waE2E.Message {
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
			client.Upload,
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
			client.Upload,
		)

	default:
		panic(fmt.Sprintf("Unsupported message type: %d", messageType))
	}
}

func contentToWaE2EMessage(
	messageType uint8,
	text string,
	mentioned []string,
	fileKind uint8,
	filePath string,
	caption *string,
	contextInfo *waE2E.ContextInfo,
	upload uploadMediaFunc,
) *waE2E.Message {
	setMentionedJIDsFromStrings(contextInfo, mentioned)
	if messageType == MessageTypeText {
		return &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			Text:        &text,
			ContextInfo: contextInfo,
		}}
	}
	message, err := buildFileMessage(context.Background(), fileKind, filePath, caption, contextInfo, upload)
	if err != nil {
		panic(err)
	}
	return message
}

func setMentionedJIDs(contextInfo *waE2E.ContextInfo, ptr *C.JID, count C.uintptr_t) {
	if contextInfo != nil {
		setMentionedJIDsFromStrings(contextInfo, mentionedJIDs(ptr, count))
	}
}

func setMentionedJIDsFromStrings(contextInfo *waE2E.ContextInfo, mentioned []string) {
	if contextInfo != nil {
		contextInfo.MentionedJID = mentioned
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
			result = append(result, parsed.ToNonAD().String())
		}
	}
	return result
}

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
	_ = sendMessage(cjid, messageType, messageContent, quoteId, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, 0)
}

//export C_SendTextMessage
func C_SendTextMessage(cjid C.JID, messageContent unsafe.Pointer, quoteId *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID C.uint64_t) C.uint8_t {
	return C.uint8_t(sendMessage(cjid, C.uint8_t(MessageTypeText), messageContent, quoteId, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, uint64(localSendID)))
}

func sendMessage(cjid C.JID, messageType C.uint8_t, messageContent unsafe.Pointer, quoteId *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID uint64) uint8 {
	if cjid == nil || messageContent == nil || client == nil || client.Store == nil || client.Store.ID == nil {
		LOG_WARN("message send rejected: client or message is unavailable")
		return 1
	}
	jid := cToJid(cjid)

	contextInfo := &waE2E.ContextInfo{}
	if quoteId != nil {
		contextInfo = quotedContextInfo(C.GoString(quoteId), C.GoString(quoteSender), C.GoString(quoteChat))
		contextInfo.QuotedMessage = quotedMessageFromContent(quoteMessageType, quoteMessageContent)
	}

	message := ContentToWaE2EMessage(messageType, messageContent, contextInfo)

	sendContext := context.Background()
	if localSendID != 0 {
		var cancel context.CancelFunc
		sendContext, cancel = optimisticTextSendContext(sendContext, optimisticTextSendTimeout)
		defer cancel()
	}
	sendResponse, err := client.SendMessage(sendContext, jid, message)
	if err != nil {
		LOG_WARN("message send failed: %v", err)
		return 1
	}

	messageInfo := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: jid, Sender: *client.Store.ID, IsFromMe: true},
		ID:            sendResponse.ID,
		Timestamp:     sendResponse.Timestamp,
	}
	LOG_INFO("Message sent: %s %s", messageInfo.ID, messageInfo.Chat)
	if localSendID != 0 {
		HandleOptimisticTextSent(localSendID, messageInfo, message)
	} else {
		HandleMessage(messageInfo, message, false)
	}
	return 0
}
