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
	bool found;
	int64_t muted_until;
	bool pinned;
	bool archived;
} ChatSettings;

typedef struct {
	uint8_t status;
	bool is_announce;
	bool is_admin;
} GroupInfoResult;

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
	uint32_t succeeded;
	uint32_t failed;
	uint8_t failure;
} ForwardResult;

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
static void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data) {
    Message copy = *data;
    copy.forwardSource = activeForwardSource;
    copy.forwardSourceLen = activeForwardSourceLen;
    hdl.callback(&copy, isSync, hdl.user_data);
}

static void callPresenceHandler(PresenceHandler hdl, JID from, bool unavailable, int64_t lastSeen) {
	hdl.callback(from, unavailable, lastSeen, hdl.user_data);
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
	"context"
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

var messageActionArrivalOrder uint64

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

//export C_GetGroupInfo
func C_GetGroupInfo(cjid C.JID) C.GroupInfoResult {
	if cjid == nil {
		return groupInfoResultToC(groupInfoClientUnavailable)
	}
	return groupInfoResultToC(fetchGroupInfo(client, cToJid(cjid).ToNonAD(), func(participant types.GroupParticipant) bool {
		return participantMatchesSelf(client, participant)
	}))
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

func isStatusProtocolContext(context *waE2E.ContextInfo) bool {
	return context.GetRemoteJID() == "status@broadcast" ||
		context.GetPosterStatusID() != "" ||
		context.StatusSourceType != nil ||
		context.StatusAttributionType != nil ||
		context.StatusAttributions != nil ||
		context.StatusAudienceMetadata != nil ||
		context.IsGroupStatus != nil
}

func forwardSourceKey(chat, sender types.JID, id types.MessageID) string {
	return chat.String() + "\x00" + sender.String() + "\x00" + string(id)
}

func HandleMessage(info types.MessageInfo, msg *waE2E.Message, isSync bool) {
	msg, viewOnceUnavailable := unavailableViewOnceMessage(msg)

	// Normalize chat and sender ids (LID→PN, broadcast→per-sender) so Rust sees canonical ids.
	if normalizedChat := GetChatId(client, &info.Chat, &info.Sender); normalizedChat != "" {
		if jid, err := types.ParseJID(normalizedChat); err == nil {
			info.Chat = jid
		}
	}
	if normalizedSender := GetUserId(client, &info.Chat, &info.Sender); normalizedSender != "" {
		if jid, err := types.ParseJID(normalizedSender); err == nil {
			info.Sender = jid
		}
	}

	chat := info.Chat
	sender := info.Sender
	if line, ok := statusProtocolReactionDiagnostic(info, msg); ok {
		emitStatusProtocolDiagnostic(messageActionDiagnostic, line)
	}
	for _, line := range statusProtocolContextDiagnostics(info, msg) {
		emitStatusProtocolDiagnostic(messageActionDiagnostic, line)
	}
	if reaction, ok := reactionEventFromMessage(info, msg); ok {
		dispatchReactionEvent(reaction)
		return
	}
	if action, ok := messageActionEventFromMessage(info, msg); ok {
		if action.kind == messageActionDelete {
			removeForwardSources(action.chat, action.targetMessageID)
		}
		dispatchMessageActionEvent(action)
		return
	}
	if !viewOnceUnavailable {
		cacheForwardSource(info, msg)
	}
	messageCallbackMu.Lock()
	defer messageCallbackMu.Unlock()
	var rawSource []byte
	if !viewOnceUnavailable {
		var err error
		rawSource, err = marshalForwardSource(msg)
		if err != nil {
			LOG_WARN("forward source serialization failed: %v", err)
			rawSource = nil
		}
	}
	var cForwardSource unsafe.Pointer
	if len(rawSource) > 0 {
		cForwardSource = C.CBytes(rawSource)
		defer C.free(cForwardSource)
	}
	C.setActiveForwardSource((*C.uint8_t)(cForwardSource), C.size_t(len(rawSource)))
	defer C.setActiveForwardSource(nil, 0)
	timestamp := info.Timestamp.Unix()
	forwarding := forwardingStateFromMessage(msg)

	cinfo := C.MessageInfo{
		id:              C.CString(info.ID),
		chat:            jidToC(chat),
		sender:          jidToC(sender),
		timestamp:       C.int64_t(timestamp),
		isFromMe:        C.bool(info.IsFromMe),
		quoteID:         nil,
		readBy:          C.uint16_t(0),
		isForwarded:     C.bool(forwarding.isForwarded),
		forwardingScore: C.uint32_t(forwarding.score),
	}

	if msg.Conversation != nil {
		ctext := C.CString(msg.GetConversation())
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
	if msg.ExtendedTextMessage != nil {
		ext_msg := msg.GetExtendedTextMessage()

		text := ext_msg.GetText()
		ctext := C.CString(text)
		defer C.free(unsafe.Pointer(ctext))

		context_info := ext_msg.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			// LOG_ERROR("asdfasdf %s", co)
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

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
	if msg.ImageMessage != nil {
		img := msg.GetImageMessage()
		if img == nil {
			LOG_ERROR("ImageMessage is nil")
			return
		}

		ext := ExtensionByType(img.GetMimetype(), ".jpg")
		caption := img.GetCaption()

		context_info := img.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		filePath := fmt.Sprintf("imgs/%s%s", info.ID, ext)

		fileId := DownloadableMessageToFileId(client, img, filePath)
		cfileId := C.CString(fileId)
		defer C.free(unsafe.Pointer(cfileId))

		cpath := C.CString(filePath)
		defer C.free(unsafe.Pointer(cpath))

		// set caption or nil
		ccaption := C.CString(caption)
		if caption == "" {
			ccaption = nil
		}
		defer C.free(unsafe.Pointer(ccaption))

		content := (*C.FileMessage)(C.malloc(C.sizeof_FileMessage))
		content.kind = C.uint8_t(FileTypeImage)
		content.path = cpath
		content.fileID = cfileId
		content.caption = ccaption
		defer C.free(unsafe.Pointer(content))

		message := C.Message{
			info:        cinfo,
			messageType: C.uint8_t(MessageTypeFile),
			message:     unsafe.Pointer(content),
		}
		C.callMessageHandler(messageHandler, C.bool(isSync), &message)
	}
	if msg.VideoMessage != nil {
		vid := msg.GetVideoMessage()
		if vid == nil {
			LOG_ERROR("VideoMessage is nil")
			return
		}

		ext := ExtensionByType(vid.GetMimetype(), ".mp4")
		caption := vid.GetCaption()

		context_info := vid.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		filePath := fmt.Sprintf("videos/%s%s", info.ID, ext)
		fileId := DownloadableMessageToFileId(client, vid, filePath)

		// Embed JPEG thumbnail into the fileId so DownloadFromFileId
		// saves it as a sidecar file (videos/<id>.jpg) alongside the video.
		if thumbnail := vid.GetJPEGThumbnail(); len(thumbnail) > 0 {
			thumbPath := strings.TrimSuffix(filePath, ext) + ".jpg"
			fileId = AddThumbnailToFileId(fileId, thumbnail, thumbPath)
		}

		cfileId := C.CString(fileId)
		defer C.free(unsafe.Pointer(cfileId))

		cpath := C.CString(filePath)
		defer C.free(unsafe.Pointer(cpath))

		ccaption := C.CString(caption)
		if caption == "" {
			ccaption = nil
		}
		defer C.free(unsafe.Pointer(ccaption))

		content := (*C.FileMessage)(C.malloc(C.sizeof_FileMessage))
		content.kind = C.uint8_t(FileTypeVideo)
		content.path = cpath
		content.fileID = cfileId
		content.caption = ccaption
		defer C.free(unsafe.Pointer(content))
		message := C.Message{
			info:        cinfo,
			messageType: C.uint8_t(MessageTypeFile),
			message:     unsafe.Pointer(content),
		}
		C.callMessageHandler(messageHandler, C.bool(isSync), &message)
	}
	if msg.AudioMessage != nil {
		audio := msg.GetAudioMessage()
		if audio == nil {
			LOG_ERROR("AudioMessage is nil")
			return
		}

		ext := ExtensionByType(audio.GetMimetype(), ".ogg")

		context_info := audio.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		filePath := fmt.Sprintf("audios/%s%s", info.ID, ext)
		fileId := DownloadableMessageToFileId(client, audio, filePath)
		cfileId := C.CString(fileId)
		defer C.free(unsafe.Pointer(cfileId))

		cpath := C.CString(filePath)
		defer C.free(unsafe.Pointer(cpath))

		content := (*C.FileMessage)(C.malloc(C.sizeof_FileMessage))
		content.kind = C.uint8_t(FileTypeAudio)
		content.path = cpath
		content.fileID = cfileId
		content.caption = nil
		defer C.free(unsafe.Pointer(content))

		message := C.Message{
			info:        cinfo,
			messageType: C.uint8_t(MessageTypeFile),
			message:     unsafe.Pointer(content),
		}
		C.callMessageHandler(messageHandler, C.bool(isSync), &message)
	}
	if msg.DocumentMessage != nil {
		doc := msg.GetDocumentMessage()
		if doc == nil {
			LOG_ERROR("DocumentMessage is nil")
			return
		}

		caption := doc.GetCaption()

		context_info := doc.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		filePath := fmt.Sprintf("docs/%s-%s", info.ID, *doc.FileName)
		fileId := DownloadableMessageToFileId(client, doc, filePath)
		cfileId := C.CString(fileId)
		defer C.free(unsafe.Pointer(cfileId))

		cpath := C.CString(filePath)
		defer C.free(unsafe.Pointer(cpath))

		ccaption := C.CString(caption)
		if caption == "" {
			ccaption = nil
		}
		defer C.free(unsafe.Pointer(ccaption))

		content := (*C.FileMessage)(C.malloc(C.sizeof_FileMessage))
		content.kind = C.uint8_t(FileTypeDocument)
		content.path = cpath
		content.fileID = cfileId
		content.caption = ccaption
		defer C.free(unsafe.Pointer(content))

		message := C.Message{
			info:        cinfo,
			messageType: C.uint8_t(MessageTypeFile),
			message:     unsafe.Pointer(content),
		}
		C.callMessageHandler(messageHandler, C.bool(isSync), &message)
	}
	if msg.StickerMessage != nil {
		sticker := msg.GetStickerMessage()
		if sticker == nil {
			LOG_ERROR("StickerMessage is nil")
			return
		}

		ext := ExtensionByType(sticker.GetMimetype(), ".webp")

		context_info := sticker.GetContextInfo()
		if context_info != nil {
			id := context_info.GetStanzaID()
			if id != "" {
				cinfo.quoteID = C.CString(id)
			}
		}

		filePath := fmt.Sprintf("stickers/%s%s", info.ID, ext)
		fileId := DownloadableMessageToFileId(client, sticker, filePath)
		cfileId := C.CString(fileId)
		defer C.free(unsafe.Pointer(cfileId))

		cpath := C.CString(filePath)
		defer C.free(unsafe.Pointer(cpath))

		content := (*C.FileMessage)(C.malloc(C.sizeof_FileMessage))
		content.kind = C.uint8_t(FileTypeSticker)
		content.path = cpath
		content.fileID = cfileId
		content.caption = nil
		defer C.free(unsafe.Pointer(content))

		message := C.Message{
			info:        cinfo,
			messageType: C.uint8_t(MessageTypeFile),
			message:     unsafe.Pointer(content),
		}
		C.callMessageHandler(messageHandler, C.bool(isSync), &message)
	}
}

// messageActionEventFromIncomingMessage also supports whatsmeow's normalized edit
// event: UnwrapRaw has already replaced Message with the edited body and Info.ID
// with the target ID. IsEdit is the library's explicit discriminator, so normal
// messages with matching text cannot be mistaken for edits.
func messageActionEventFromIncomingMessage(evt *events.Message) (messageActionEvent, bool) {
	if evt == nil {
		return messageActionEvent{}, false
	}
	rawProbe := messageActionProbeFromMessage(evt.RawMessage, "raw")
	if action, ok, reason := messageActionEventFromProbe(evt.Info, rawProbe); ok {
		if sourceID := evt.SourceWebMsg.GetKey().GetID(); sourceID != "" {
			action.actionID = sourceID
		}
		messageActionStructuralDiagnostic(evt, "raw_classified", "")
		return action, true
	} else if rawProbe.hasActionProtocol() {
		messageActionStructuralDiagnostic(evt, "raw_miss", reason)
		return messageActionEvent{}, false
	}
	if !evt.IsEdit {
		messageActionStructuralDiagnostic(evt, "signal_miss", "protocol_absent")
		return messageActionEvent{}, false
	}
	// Only ParseWebMessage rewrites Info.ID to the target. Live edit envelopes keep
	// Info.ID as the action ID, so never use this fallback without history source data.
	if evt.SourceWebMsg == nil || evt.SourceWebMsg.GetKey().GetID() == "" || evt.Info.ID == "" {
		messageActionStructuralDiagnostic(evt, "normalized_miss", "unproven_target")
		return messageActionEvent{}, false
	}
	replacement, ok := replacementText(evt.Message)
	if !ok {
		messageActionStructuralDiagnostic(evt, "normalized_miss", "missing_replacement")
		return messageActionEvent{}, false
	}
	action := messageActionEvent{
		actionID:        evt.SourceWebMsg.GetKey().GetID(),
		chat:            evt.Info.Chat.String(),
		sender:          evt.Info.Sender.String(),
		targetMessageID: evt.Info.ID,
		replacement:     replacement,
		occurredAt:      evt.Info.Timestamp.Unix(),
		arrivalOrder:    atomic.AddUint64(&messageActionArrivalOrder, 1),
		kind:            messageActionEdit,
	}
	messageActionStructuralDiagnostic(evt, "normalized_classified", "")
	return action, true
}

func messageActionKindName(kind uint8) string {
	if kind == messageActionEdit {
		return "edit"
	}
	return "delete"
}

//export C_TestEmitPresenceEvent
func C_TestEmitPresenceEvent(from *C.char, unavailable C.bool, lastSeen C.int64_t) {
	C.callPresenceHandler(presenceHandler, from, unavailable, lastSeen)
}

//export C_TestEmitPresenceEventsConcurrently
func C_TestEmitPresenceEventsConcurrently(from *C.char, count C.uint32_t) {
	var wait sync.WaitGroup
	for index := uint32(0); index < uint32(count); index++ {
		wait.Add(1)
		go func(lastSeen uint32) {
			defer wait.Done()
			C.callPresenceHandler(presenceHandler, from, C.bool(false), C.int64_t(lastSeen))
		}(index)
	}
	wait.Wait()
}

//export C_ForwardMessage
func C_ForwardMessage(sourceID *C.char, sourceChat C.JID, sourceSender C.JID, sourceIsFromMe C.bool, destinations **C.char, destinationCount C.size_t, forwardSource *C.uint8_t, forwardSourceLen C.size_t) C.ForwardResult {
	if sourceID == nil || sourceChat == nil || sourceSender == nil || destinations == nil || destinationCount == 0 {
		return C.ForwardResult{}
	}
	rawDestinations := unsafe.Slice(destinations, int(destinationCount))
	destinationStrings := make([]string, 0, len(rawDestinations))
	for _, destination := range rawDestinations {
		if destination == nil {
			return C.ForwardResult{failed: C.uint32_t(destinationCount)}
		}
		destinationStrings = append(destinationStrings, C.GoString(destination))
	}
	return forwardMessages(
		C.GoString(sourceID),
		C.GoString(sourceChat),
		C.GoString(sourceSender),
		bool(sourceIsFromMe),
		destinationStrings,
		forwardingSourceBytes(forwardSource, forwardSourceLen),
	).cResult()
}

//export C_GetChatSettings
func C_GetChatSettings(cjid C.JID) C.ChatSettings {
	ctx := context.Background()
	jid := cToJid(cjid).ToNonAD()
	settings, err := lookupChatSettings(
		ctx,
		jid,
		client.Store.ChatSettings.GetChatSettings,
		client.Store.LIDs.GetLIDForPN,
		client.Store.LIDs.GetPNForLID,
	)
	if err != nil {
		LOG_WARN("failed to get chat settings for %s: %v", jid, err)
		return C.ChatSettings{}
	}
	payload := chatSettingsPayloadFrom(settings)
	return C.ChatSettings{
		found:       C.bool(payload.found),
		muted_until: C.int64_t(payload.mutedUntil),
		pinned:      C.bool(payload.pinned),
		archived:    C.bool(payload.archived),
	}
}

// Resolve a user JID (typically a group participant) to its canonical
// direct-conversation id. Direct chats are stored under the phone number
// (PN); a group participant may be a LID, so we map LID→PN when known so the
// private reply opens the real chat instead of an empty LID-keyed thread.
// Non-LID personal JIDs pass through unchanged. Returns NULL when the
// client is not ready or the JID cannot be parsed.
//
//export C_ResolveDmChatId
func C_ResolveDmChatId(cjid C.JID) *C.char {
	if client == nil {
		return nil
	}
	jid, err := types.ParseJID(C.GoString(cjid))
	if err != nil || jid.IsEmpty() {
		return nil
	}
	return C.CString(GetChatId(client, &jid, nil))
}

//export C_FreeResolveDmChatId
func C_FreeResolveDmChatId(value *C.char) {
	C.free(unsafe.Pointer(value))
}

//export C_MarkAsRead
func C_MarkAsRead(msg_id *C.char, chat_jid C.JID, sender_jid C.JID) {
	ctx := context.Background()
	timeRead := time.Now()

	msgIds := []types.MessageID{
		C.GoString(msg_id),
	}

	client.MarkRead(
		ctx,
		msgIds,
		timeRead,
		cToJid(chat_jid),
		cToJid(sender_jid),
	)
}

func main() {} // Required for CGO
