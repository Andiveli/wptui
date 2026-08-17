package main

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdio.h>

typedef const char* JID;

typedef struct {
	bool found;
	const char* first_name;
	const char* full_name;
	const char* push_name;
	const char* business_name;
} Contact;

typedef struct {
	JID jid;
	const char* name;
} ContactEntry;

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
	ContactEntry* entries;
	uint32_t size;
} GetContactsResult;

typedef struct {
	uint8_t status;
	char* picture_id;
	char* picture_type;
	uint8_t* data;
	uint32_t size;
} ProfilePictureResult;

typedef struct {
	char* id;
	JID chat;
	JID sender;
	int64_t timestamp;
	bool isFromMe;
	char* quoteID;
	uint16_t readBy;
	bool isForwarded;
	uint32_t forwardingScore;
} MessageInfo;

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
	MessageInfo info;
	uint8_t messageType;
	void* message;
	uint8_t* forwardSource;
	size_t forwardSourceLen;
} Message;

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

typedef struct {
	uint8_t kind;
	void* data;
} Event;

typedef void (*EventCallback)(const Event*, void*);
typedef struct {
	EventCallback callback;
	void* user_data;
} EventHandler;
static void callEventCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
typedef void (*QrCallback)(const char*, void*);
static void callQrCallback(QrCallback cb, const char* code, void* user_data) {
	cb(code, user_data);
}

typedef void (*MessageHandlerCallback)(const Message*, bool, void*);
typedef struct {
	MessageHandlerCallback callback;
	void* user_data;
} MessageHandler;
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

typedef void (*PresenceHandlerCallback)(JID, bool, int64_t, void*);
typedef struct {
	PresenceHandlerCallback callback;
	void* user_data;
} PresenceHandler;
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

typedef void (*LogHandlerCallback)(const char*, uint8_t, void*);
typedef struct {
	LogHandlerCallback callback;
	void* user_data;
} LogHandler;
static void callLogInfo(LogHandler hdl, const char* msg, uint8_t level) {
	hdl.callback(msg, level, hdl.user_data);
}
*/
import "C"
import (
	"context"
	"fmt"
	"mime"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

var logHandler C.LogHandler
var messageHandler C.MessageHandler
var eventHandler C.EventHandler
var presenceHandler C.PresenceHandler

var messageActionArrivalOrder uint64

var messageCallbackMu sync.Mutex

const (
	forwardFailureNone uint8 = iota
	forwardFailureSourceUnavailable
	forwardFailureInvalidSource
	forwardFailureInvalidDestination
	forwardFailureSendFailed
)

func messageActionDiagnostic(msg string, args ...any) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	LOG_WARN("MESSAGE_ACTION_DIAG "+msg, args...)
}

func emitStatusProtocolDiagnostic(emit func(string, ...any), entry string) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	emit("%s", entry)
}

func messageActionCensusDiagnostic(rawEvt any) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	messageActionCensusAppend(messageActionCensusLine(rawEvt))
}

func messageActionCensusLine(rawEvt any) string {
	eventType := strings.NewReplacer("*", "", ".", "_").Replace(fmt.Sprintf("%T", rawEvt))
	eventType = strings.ToLower(eventType)
	switch evt := rawEvt.(type) {
	case *events.Message:
		return messageActionMessageCensusLine(eventType, evt)
	case *events.AppState:
		return fmt.Sprintf("event_type=%s subtype=%s", eventType, appStateCensusSubtype(evt))
	case *events.AppStateSyncComplete:
		return fmt.Sprintf("event_type=%s subtype=sync_complete app_state=%s", eventType, safeCensusName(string(evt.Name)))
	case *events.AppStateSyncError:
		return fmt.Sprintf("event_type=%s subtype=sync_error app_state=%s", eventType, safeCensusName(string(evt.Name)))
	case *events.Receipt:
		return fmt.Sprintf("event_type=%s subtype=receipt_%s", eventType, receiptCensusSubtype(evt.Type))
	case *events.UndecryptableMessage:
		return fmt.Sprintf("event_type=%s subtype=undecryptable_%s", eventType, safeCensusName(string(evt.UnavailableType)))
	default:
		return fmt.Sprintf("event_type=%s", eventType)
	}
}

func receiptCensusSubtype(receiptType types.ReceiptType) string {
	switch receiptType {
	case types.ReceiptTypeDelivered:
		return "delivered"
	case types.ReceiptTypeSender:
		return "sender"
	case types.ReceiptTypeRetry:
		return "retry"
	case types.ReceiptTypeRead:
		return "read"
	case types.ReceiptTypeReadSelf:
		return "read_self"
	case types.ReceiptTypePlayed:
		return "played"
	default:
		return "other"
	}
}

func messageActionMessageCensusLine(eventType string, evt *events.Message) string {
	raw := messageActionProbeFromMessage(evt.RawMessage, "raw")
	normalized := messageActionProbeFromMessage(evt.Message, "message")
	source := messageActionProbe{}
	var sourceMessage *waE2E.Message
	if evt.SourceWebMsg != nil {
		sourceMessage = evt.SourceWebMsg.GetMessage()
		source = messageActionProbeFromMessage(sourceMessage, "source")
	}
	selected := raw
	if selected.protocol == nil {
		selected = normalized
	}
	if selected.protocol == nil {
		selected = source
	}
	protocolType := "none"
	protocolKeyID := false
	if selected.protocol != nil {
		protocolType = safeCensusName(selected.protocol.GetType().String())
		protocolKeyID = selected.protocol.GetKey().GetID() != ""
	}
	secret := raw.secretEncrypted
	if secret == nil {
		secret = normalized.secretEncrypted
	}
	if secret == nil {
		secret = source.secretEncrypted
	}
	secretType := "none"
	secretTypeNumber := 0
	secretTargetPresent := false
	secretTargetID := "<missing>"
	secretPayloadLength := 0
	secretIVLength := 0
	if secret != nil {
		secretType = safeCensusName(secret.GetSecretEncType().String())
		secretTypeNumber = int(secret.GetSecretEncType())
		secretTargetPresent = secret.GetTargetMessageKey() != nil
		if secret.GetTargetMessageKey().GetID() != "" {
			secretTargetID = messageActionIdentifier(secret.GetTargetMessageKey().GetID())
		}
		secretPayloadLength = len(secret.GetEncPayload())
		secretIVLength = len(secret.GetEncIV())
	}
	sourceKey := "<missing>"
	if evt.SourceWebMsg != nil && evt.SourceWebMsg.GetKey().GetID() != "" {
		sourceKey = messageActionIdentifier(evt.SourceWebMsg.GetKey().GetID())
	}
	// whatsmeow has no IsHistory flag; SourceWebMsg marks parsed history or an unavailable-message response.
	return fmt.Sprintf("event_type=%s is_edit=%t is_history=%t info_id=%s chat=%s sender=%s roots=raw:%t,message:%t,source_web_msg:%t raw_kinds=%s message_kinds=%s source_kinds=%s wrappers=%s protocol_present=%t protocol_type=%s protocol_key_id=%t source_key=%s secret_enc_type=%s secret_enc_type_number=%d secret_target_present=%t secret_target_id=%s secret_payload_length=%d secret_iv_length=%d decrypt_result=not_attempted decrypted_content_kind=none", eventType, evt.IsEdit, evt.SourceWebMsg != nil, messageActionIdentifier(evt.Info.ID), messageActionIdentifier(evt.Info.Chat.String()), messageActionIdentifier(evt.Info.Sender.String()), evt.RawMessage != nil, evt.Message != nil, evt.SourceWebMsg != nil, messageCensusKinds(evt.RawMessage), messageCensusKinds(evt.Message), messageCensusKinds(sourceMessage), messageCensusWrappers(evt.RawMessage, "raw"), selected.protocol != nil, protocolType, protocolKeyID, sourceKey, secretType, secretTypeNumber, secretTargetPresent, secretTargetID, secretPayloadLength, secretIVLength)
}

func appStateCensusSubtype(evt *events.AppState) string {
	switch {
	case evt.GetDeleteMessageForMeAction() != nil:
		return "delete_message_for_me"
	case evt.GetStarAction() != nil:
		return "star"
	case evt.GetLabelAssociationAction() != nil:
		return "label_association"
	default:
		return "other"
	}
}

func safeCensusName(value string) string {
	value = strings.ToLower(value)
	value = strings.NewReplacer(".", "_", "-", "_", " ", "_").Replace(value)
	return strings.Map(func(character rune) rune {
		if character >= 'a' && character <= 'z' || character >= '0' && character <= '9' || character == '_' {
			return character
		}
		return -1
	}, value)
}

func messageActionIdentifier(identifier string) string {
	if identifier == "" {
		return "<missing>"
	}
	hash := uint64(0xcbf29ce484222325)
	for _, byte := range []byte(identifier) {
		hash ^= uint64(byte)
		hash *= 0x100000001b3
	}
	return fmt.Sprintf("<id:%08x>", hash)
}

func LOG_LEVEL(level int, msg string, args ...any) {
	formatted := fmt.Sprintf(msg, args...)
	if os.Getenv("WPTUI_LOGS_STDERR") == "1" {
		// Debug aid for headless capture: mirror every Go log to stderr so a
		// run like `WPTUI_LOGS_STDERR=1 target/debug/wp-tui 2>go.log` lets the
		// user read diagnostics after closing the TUI (Ctrl+L panel is broken).
		fmt.Fprintf(os.Stderr, "[go %d] %s\n", level, formatted)
	}
	if logHandler.callback == nil {
		return
	}
	cmsg := C.CString(formatted)
	defer C.free(unsafe.Pointer(cmsg))
	C.callLogInfo(logHandler, cmsg, C.uint8_t(level))
}

func LOG_ERROR(msg string, args ...any) {
	LOG_LEVEL(0, msg, args...)
}
func LOG_WARN(msg string, args ...any) {
	LOG_LEVEL(1, msg, args...)
}
func LOG_INFO(msg string, args ...any) {
	LOG_LEVEL(2, msg, args...)
}
func LOG_DEBUG(msg string, args ...any) {
	LOG_LEVEL(4, msg, args...)
}

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

// Convert Go JID to C JID
func jidToC(jid types.JID) C.JID {
	return C.CString(jid.ToNonAD().String())
	// return C.CString(jid.User + "@" + jid.Server)
}

// Convert C JID to Go JID
func cToJid(cjid C.JID) types.JID {
	jid, err := types.ParseJID(C.GoString(cjid))
	if err != nil {
		panic(err)
	}
	return jid
}

//export C_SetLogHandler
func C_SetLogHandler(callback C.LogHandlerCallback, data unsafe.Pointer) {
	logHandler = C.LogHandler{
		callback:  callback,
		user_data: data,
	}
}

//export C_SetEventHandler
func C_SetEventHandler(callback C.EventCallback, data unsafe.Pointer) {
	eventHandler = C.EventHandler{
		callback:  callback,
		user_data: data,
	}
}

//export C_SetMessageHandler
func C_SetMessageHandler(callback C.MessageHandlerCallback, data unsafe.Pointer) {
	messageHandler = C.MessageHandler{
		callback:  callback,
		user_data: data,
	}
}

//export C_SetPresenceHandler
func C_SetPresenceHandler(callback C.PresenceHandlerCallback, data unsafe.Pointer) {
	presenceHandler = C.PresenceHandler{
		callback:  callback,
		user_data: data,
	}
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

type uploadMediaFunc func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error)

// buildFileMessage maps one FFI file payload into its WhatsApp message without
// requiring a connected client. Keeping upload injectable lets the mapping stay
// deterministic and testable with fake upload responses.
func buildFileMessage(ctx context.Context, kind uint8, filePath string, caption *string, contextInfo *waE2E.ContextInfo, upload uploadMediaFunc) (*waE2E.Message, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("read file %s: %w", filePath, err)
	}
	mimetype := mime.TypeByExtension(filepath.Ext(filePath))

	uploadedMessage := func(mediaType whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
		uploaded, uploadErr := upload(ctx, data, mediaType)
		if uploadErr != nil {
			return whatsmeow.UploadResponse{}, fmt.Errorf("upload file: %w", uploadErr)
		}
		return uploaded, nil
	}

	switch kind {
	case FileTypeImage:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaImage)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{ImageMessage: &waE2E.ImageMessage{
			Caption:       caption,
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeVideo:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaVideo)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{VideoMessage: &waE2E.VideoMessage{
			Caption:       caption,
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeAudio:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaAudio)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{AudioMessage: &waE2E.AudioMessage{
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeDocument:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaDocument)
		if uploadErr != nil {
			return nil, uploadErr
		}
		fileName := filepath.Base(filePath)
		return &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{
			Caption:       caption,
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			FileName:      proto.String(fileName),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeSticker:
		// WhatsApp stickers use the image media encryption keys.
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaImage)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{StickerMessage: &waE2E.StickerMessage{
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	default:
		return nil, fmt.Errorf("unsupported file type: %v", kind)
	}
}

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

func AddEventHandlers() {
	client.AddEventHandler(func(rawEvt any) {
		messageActionCensusDiagnostic(rawEvt)
		switch evt := rawEvt.(type) {
		case *events.Connected:
			handleConnected(
				client.SendPresence,
				dispatchConnectedEvent,
				LOG_WARN,
			)

		case *events.Presence:
			dispatchPresenceEvent(evt, client.Store.LIDs.GetPNForLID, rawPresenceProbe.record, rawPresenceProbe.update, func(from string, unavailable bool, lastSeen int64) {
				cFrom := C.CString(from)
				defer C.free(unsafe.Pointer(cFrom))
				C.callPresenceHandler(presenceHandler, cFrom, C.bool(unavailable), C.int64_t(lastSeen))
			})

		case *events.MarkChatAsRead:
			LOG_DEBUG("MarkChatAsRead %v", evt.JID)

		case *events.AppStateSyncComplete:
			dispatchAppStateSyncComplete(evt)

		case *events.Message:
			dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage)

		case *events.Receipt:
			if receipt, ok := receiptEventFromEvent(evt); ok {
				LOG_DEBUG("%#v was read by %s at %s", evt.MessageIDs, evt.SourceString(), evt.Timestamp)
				dispatchReceiptEvent(receipt)
			}

		case *events.HistorySync:
			dispatchHistorySync(
				evt,
				client.DangerousInternals().StoreHistoricalMessageSecrets,
				client.ParseWebMessage,
				func(parsed *events.Message) {
					dispatchIncomingMessage(parsed, dispatchMessageActionEvent, func(info types.MessageInfo, message *waE2E.Message, _ bool) {
						HandleMessage(info, message, true)
					})
				},
			)
		}
	})
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

//export C_GetContacts
func C_GetContacts() C.GetContactsResult {
	if client == nil || client.Store == nil {
		return contactEntriesToC(nil)
	}
	ctx := context.Background()
	entries := lookupContactEntries(ctx, client)

	// Groups remain in this bridge wrapper; contacts.go owns only contact lookup.
	groups, err := client.GetJoinedGroups(ctx)
	if err != nil {
		panic(err)
	}
	for _, group := range groups {
		entries = append(entries, contactEntry{jid: group.JID, name: group.GroupName.Name})
	}
	return contactEntriesToC(entries)
}

//export C_GetProfilePicture
func C_GetProfilePicture(jid *C.char) C.ProfilePictureResult {
	if jid == nil {
		return C.ProfilePictureResult{status: C.uint8_t(profilePictureStatusInvalidJID)}
	}
	if client == nil {
		return C.ProfilePictureResult{status: C.uint8_t(profilePictureStatusClientUnavailable)}
	}
	ctx, cancel := context.WithTimeout(context.Background(), profilePictureTimeout)
	defer cancel()
	return profilePictureToC(fetchProfilePicture(ctx, C.GoString(jid), client.GetProfilePictureInfo, downloadProfilePicture))
}

//export C_FreeProfilePicture
func C_FreeProfilePicture(result C.ProfilePictureResult) {
	C.free(unsafe.Pointer(result.picture_id))
	C.free(unsafe.Pointer(result.picture_type))
	C.free(unsafe.Pointer(result.data))
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
