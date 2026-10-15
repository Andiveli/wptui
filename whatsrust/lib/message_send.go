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

	"go.mau.fi/whatsmeow"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

type statusSendResult uint8

const (
	statusSendResultSent statusSendResult = iota
	statusSendResultUnsupportedContent
	statusSendResultInvalidContent
	statusSendResultClientUnavailable
	statusSendResultMediaPreparationFailed
	statusSendResultSendFailed
)

type statusSendRequest struct {
	messageType uint8
	text        string
	fileKind    uint8
	filePath    string
	caption     *string
}

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

//export C_SendStatusMessage
func C_SendStatusMessage(messageType C.uint8_t, messageContent unsafe.Pointer) C.uint8_t {
	return C_SendStatusMessageWithLocalID(messageType, messageContent, 0)
}

//export C_SendStatusMessageWithLocalID
func C_SendStatusMessageWithLocalID(messageType C.uint8_t, messageContent unsafe.Pointer, localSendID C.uint64_t) C.uint8_t {
	request, result := statusSendRequestFromC(messageType, messageContent)
	if result != statusSendResultSent {
		return C.uint8_t(result)
	}
	return C.uint8_t(sendStatusMessageWithLocalID(request, uint64(localSendID)))
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

func statusSendRequestFromC(messageType C.uint8_t, messageContent unsafe.Pointer) (statusSendRequest, statusSendResult) {
	if messageContent == nil {
		return statusSendRequest{}, statusSendResultInvalidContent
	}
	switch messageType {
	case C.uint8_t(MessageTypeText):
		textMessage := (*C.SendTextMessage)(messageContent)
		if textMessage.text == nil {
			return statusSendRequest{}, statusSendResultInvalidContent
		}
		return statusSendRequest{messageType: MessageTypeText, text: C.GoString(textMessage.text)}, statusSendResultSent
	case C.uint8_t(MessageTypeFile):
		fileMessage := (*C.SendFileMessage)(messageContent)
		if fileMessage.path == nil {
			return statusSendRequest{}, statusSendResultInvalidContent
		}
		if uint8(fileMessage.kind) != FileTypeImage && uint8(fileMessage.kind) != FileTypeVideo {
			return statusSendRequest{}, statusSendResultUnsupportedContent
		}
		request := statusSendRequest{
			messageType: MessageTypeFile,
			fileKind:    uint8(fileMessage.kind),
			filePath:    C.GoString(fileMessage.path),
		}
		if fileMessage.caption != nil {
			caption := C.GoString(fileMessage.caption)
			request.caption = &caption
		}
		return request, statusSendResultSent
	default:
		return statusSendRequest{}, statusSendResultUnsupportedContent
	}
}

func sendStatusMessageWithLocalID(request statusSendRequest, localSendID uint64) statusSendResult {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil || clientSnapshot.Store.ID == nil {
		LOG_WARN("status send rejected: client is unavailable")
		return statusSendResultClientUnavailable
	}
	return sendStatusRequestWithLocalID(context.Background(), request, clientSnapshot, clientSnapshot.Upload, requestSendMessage, localSendID, HandleOptimisticTextSent)
}

func sendStatusRequest(ctx context.Context, request statusSendRequest, clientSnapshot *whatsmeow.Client, upload uploadMediaFunc, send sendMessageRequest) statusSendResult {
	return sendStatusRequestWithLocalID(ctx, request, clientSnapshot, upload, send, 0, nil)
}

func sendStatusRequestWithLocalID(ctx context.Context, request statusSendRequest, clientSnapshot *whatsmeow.Client, upload uploadMediaFunc, send sendMessageRequest, localSendID uint64, onSent func(uint64, types.MessageInfo, *waE2E.Message)) statusSendResult {
	var message *waE2E.Message
	switch request.messageType {
	case MessageTypeText:
		message = &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: &request.text}}
	case MessageTypeFile:
		if request.fileKind != FileTypeImage && request.fileKind != FileTypeVideo {
			return statusSendResultUnsupportedContent
		}
		if upload == nil {
			return statusSendResultMediaPreparationFailed
		}
		var err error
		message, err = buildFileMessage(ctx, request.fileKind, request.filePath, request.caption, nil, upload)
		if err != nil {
			LOG_WARN("status media preparation failed: %v", err)
			return statusSendResultMediaPreparationFailed
		}
	default:
		return statusSendResultUnsupportedContent
	}
	response, err := send(clientSnapshot, ctx, types.StatusBroadcastJID, message)
	if err != nil {
		LOG_WARN("status send failed: %v", err)
		return statusSendResultSendFailed
	}
	if localSendID != 0 && response.ID == "" {
		LOG_WARN("status send returned no canonical ID; outcome is uncertain")
		return statusSendResultSendFailed
	}
	if localSendID != 0 && onSent != nil {
		info := types.MessageInfo{
			MessageSource: types.MessageSource{Chat: types.StatusBroadcastJID, Sender: *clientSnapshot.Store.ID, IsFromMe: true},
			ID:            response.ID, Timestamp: response.Timestamp,
		}
		onSent(localSendID, info, message)
	}
	return statusSendResultSent
}

func sendNormalTextRequest(request textSendRequest) uint8 {
	request.localSendID = 0
	return sendOutboundRequest(request)
}

func sendOptimisticTextRequest(request textSendRequest) uint8 {
	return sendOutboundRequest(request)
}
