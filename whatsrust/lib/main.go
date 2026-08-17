package main

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>
#include "callback_log_registration.h"

typedef struct {
	bool found;
	const char* first_name;
	const char* full_name;
	const char* push_name;
	const char* business_name;
} Contact;

typedef struct {
	char* text;
} TextMessage;

typedef struct {
	uint8_t kind;
	char* path;
	char* fileID;
	char* caption;
} FileMessage;

typedef struct {
	uint8_t kind;
	JID id;
	char* const* messageIDs;
	size_t size;
} ReceiptEvent;

typedef struct {
	JID chat;
	char* targetMessageID;
	JID participant;
	char* text;
	bool isFromMe;
} ReactionEvent;

typedef struct {
	char* actionID;
	JID chat;
	JID sender;
	char* targetMessageID;
	char* replacement;
	int64_t occurredAt;
	uint64_t arrivalOrder;
	uint8_t kind;
} MessageActionEvent;

typedef struct {
	JID chat;
	int64_t lastMessageTime;
} ChatEvent;

typedef struct {
	uint8_t status;
} LogoutResultEvent;

static void callEventCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
typedef void (*QrCallback)(const char*, void*);
static void callQrCallback(QrCallback cb, const char* code, void* user_data) {
	cb(code, user_data);
}

static uint8_t* activeForwardSource;
static size_t activeForwardSourceLen;
static void setActiveForwardSource(uint8_t* source, size_t length) {
    activeForwardSource = source;
    activeForwardSourceLen = length;
}
void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data) {
    Message copy = *data;
    copy.forwardSource = activeForwardSource;
    copy.forwardSourceLen = activeForwardSourceLen;
    hdl.callback(&copy, isSync, hdl.user_data);
}

typedef void (*HistorySyncCallback)(uint32_t, void*);
typedef struct {
	HistorySyncCallback callback;
	void* user_data;
} HistorySyncHandler;
static void callHistorySync(HistorySyncHandler hdl, uint32_t percent) {
	hdl.callback(percent, hdl.user_data);
}

*/
import "C"
import (
	"sync"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

var messageCallbackMu sync.Mutex

const (
	forwardFailureNone uint8 = iota
	forwardFailureSourceUnavailable
	forwardFailureInvalidSource
	forwardFailureInvalidDestination
	forwardFailureSendFailed
)

// GetSelfId returns the current user's JID string for comparison (e.g. broadcast sender).
func GetSelfId(client *whatsmeow.Client) string {
	if client == nil || client.Store == nil || client.Store.ID == nil {
		return ""
	}
	return StrFromJid(*client.Store.ID)
}

type messageCallbackMetadata struct {
	id         string
	chat       types.JID
	sender     types.JID
	timestamp  int64
	isFromMe   bool
	forwarding forwardingState
}

func messageCallbackMetadataFrom(info types.MessageInfo, msg *waE2E.Message) messageCallbackMetadata {
	return messageCallbackMetadata{
		id:         info.ID,
		chat:       info.Chat,
		sender:     info.Sender,
		timestamp:  info.Timestamp.Unix(),
		isFromMe:   info.IsFromMe,
		forwarding: forwardingStateFromMessage(msg),
	}
}

type messageCallback struct {
	info        C.MessageInfo
	forwardData unsafe.Pointer
}

func beginMessageCallback(info types.MessageInfo, msg *waE2E.Message, rawSource []byte) *messageCallback {
	messageCallbackMu.Lock()
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
		timestamp:       C.int64_t(metadata.timestamp),
		isFromMe:        C.bool(metadata.isFromMe),
		quoteID:         nil,
		readBy:          C.uint16_t(0),
		isForwarded:     C.bool(metadata.forwarding.isForwarded),
		forwardingScore: C.uint32_t(metadata.forwarding.score),
	}
	return callback
}

func (callback *messageCallback) close() {
	C.setActiveForwardSource(nil, 0)
	if callback.forwardData != nil {
		C.free(callback.forwardData)
	}
	messageCallbackMu.Unlock()
}

const (
	EventTypeSyncProgress         = 0
	EventTypeAppStateSyncComplete = 1
	EventTypeReceipt              = 2
	EventTypeReaction             = 3
	// Event type 4 is reserved for the removed multiplexed Presence event.
	EventTypeConnected     = 5
	EventTypeMessageAction = 6
	EventTypeChat          = 7
	EventTypeLogoutResult  = 8
)

const (
	MessageTypeText = iota
	MessageTypeFile
)

const (
	FileTypeImage = iota
	FileTypeVideo
	FileTypeAudio
	FileTypeDocument
	FileTypeSticker
)

func emitTextMessage(cinfo C.MessageInfo, text string, isSync bool) {
	ctext := C.CString(text)
	defer C.free(unsafe.Pointer(ctext))

	content := (*C.TextMessage)(C.malloc(C.sizeof_TextMessage))
	content.text = ctext
	defer C.free(unsafe.Pointer(content))

	message := C.Message{
		info:        cinfo,
		messageType: C.uint8_t(MessageTypeText),
		message:     unsafe.Pointer(content),
	}
	C.callMessageHandler(messageHandler, C.bool(isSync), &message)
}

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

		emitTextMessage(cinfo, ext_msg.GetText(), isSync)
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

func messageActionKindName(kind uint8) string {
	if kind == messageActionEdit {
		return "edit"
	}
	return "delete"
}

func main() {} // Required for CGO
