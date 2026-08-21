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

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

type sendMessageRequest func(context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error)

var requestSendMessage sendMessageRequest = func(ctx context.Context, jid types.JID, message *waE2E.Message) (whatsmeow.SendResponse, error) {
	return client.SendMessage(ctx, jid, message)
}

var normalMessageCallback = HandleMessage
var optimisticTextSentCallback = HandleOptimisticTextSent

type textSendQuote struct {
	stanzaID    string
	participant string
	remoteJID   string
	content     *waE2E.Message
}

type textSendRequest struct {
	messageType   uint8
	chat         types.JID
	text         string
	mentionedJIDs []string
	fileKind     uint8
	filePath     string
	caption      *string
	quote        *textSendQuote
	localSendID  uint64
}

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
			result = append(result, parsed.String())
		}
	}
	return normalizeMentionedJIDs(result)
}

func normalizeMentionedJIDs(mentioned []string) []string {
	result := make([]string, 0, len(mentioned))
	for _, value := range mentioned {
		parsed, err := types.ParseJID(value)
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
	request, ok := textSendRequestFromC(cjid, messageType, messageContent, quoteId, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, 0)
	if ok {
		_ = sendNormalTextRequest(request)
	}
}

//export C_SendTextMessage
func C_SendTextMessage(cjid C.JID, messageContent unsafe.Pointer, quoteId *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID C.uint64_t) C.uint8_t {
	request, ok := textSendRequestFromC(cjid, C.uint8_t(MessageTypeText), messageContent, quoteId, quoteSender, quoteChat, quoteMessageType, quoteMessageContent, uint64(localSendID))
	if !ok {
		return 1
	}
	return C.uint8_t(sendOptimisticTextRequest(request))
}

func textSendRequestFromC(cjid C.JID, messageType C.uint8_t, messageContent unsafe.Pointer, quoteID *C.char, quoteSender C.JID, quoteChat C.JID, quoteMessageType C.uint8_t, quoteMessageContent unsafe.Pointer, localSendID uint64) (textSendRequest, bool) {
	if cjid == nil || messageContent == nil {
		return textSendRequest{}, false
	}
	request := textSendRequest{
		messageType:   uint8(messageType),
		chat:          cToJid(cjid),
		localSendID:   localSendID,
	}
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
			stanzaID:    C.GoString(quoteID),
			participant: C.GoString(quoteSender),
			remoteJID:   C.GoString(quoteChat),
			content:     quotedMessageFromContent(quoteMessageType, quoteMessageContent),
		}
	}
	return request, true
}

func sendNormalTextRequest(request textSendRequest) uint8 {
	request.localSendID = 0
	return sendTextRequest(request)
}

func sendOptimisticTextRequest(request textSendRequest) uint8 {
	if request.localSendID == 0 {
		return sendNormalTextRequest(request)
	}
	return sendTextRequest(request)
}

func sendTextRequest(request textSendRequest) uint8 {
	if client == nil || client.Store == nil || client.Store.ID == nil {
		LOG_WARN("message send rejected: client or message is unavailable")
		return 1
	}
	contextInfo := &waE2E.ContextInfo{}
	if request.quote != nil {
		contextInfo = quotedContextInfo(request.quote.stanzaID, request.quote.participant, request.quote.remoteJID)
		contextInfo.QuotedMessage = request.quote.content
	}
	message := contentToWaE2EMessage(request.messageType, request.text, normalizeMentionedJIDs(request.mentionedJIDs), request.fileKind, request.filePath, request.caption, contextInfo, client.Upload)

	sendContext := context.Background()
	if request.localSendID != 0 {
		var cancel context.CancelFunc
		sendContext, cancel = optimisticTextSendContext(sendContext, optimisticTextSendTimeout)
		defer cancel()
	}
	sendResponse, err := requestSendMessage(sendContext, request.chat, message)
	if err != nil {
		LOG_WARN("message send failed: %v", err)
		return 1
	}

	messageInfo := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: request.chat, Sender: *client.Store.ID, IsFromMe: true},
		ID:            sendResponse.ID,
		Timestamp:     sendResponse.Timestamp,
	}
	LOG_INFO("Message sent: %s %s", messageInfo.ID, messageInfo.Chat)
	if request.localSendID != 0 {
		optimisticTextSentCallback(request.localSendID, messageInfo, message)
	} else {
		normalMessageCallback(messageInfo, message, false)
	}
	return 0
}
