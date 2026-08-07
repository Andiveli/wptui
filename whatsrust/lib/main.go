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
	"bytes"
	"context"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"math"
	"mime"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unsafe"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protoreflect"

	_ "github.com/mattn/go-sqlite3"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/proto/waHistorySync"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	waLog "go.mau.fi/whatsmeow/util/log"
)

var client *whatsmeow.Client
var qrChan <-chan whatsmeow.QRChannelItem

var logHandler C.LogHandler
var messageHandler C.MessageHandler
var eventHandler C.EventHandler
var presenceHandler C.PresenceHandler

const maxRawPresenceDiagnosticEntries = 50
const presenceNormalizationTimeout = 250 * time.Millisecond

type rawPresenceDiagnosticEntry struct {
	sequence        uint64
	server          string
	unavailable     bool
	lastSeenPresent bool
	normalized      string
	normalization   string
	dispatch        string
}

type rawPresenceDiagnostics struct {
	mu      sync.Mutex
	enabled atomic.Bool
	total   uint64
	entries []rawPresenceDiagnosticEntry
}

var rawPresenceProbe rawPresenceDiagnostics

func classifyPresenceServer(server string) string {
	switch server {
	case types.DefaultUserServer:
		return "s.whatsapp.net"
	case types.HiddenUserServer:
		return "lid"
	default:
		return "other"
	}
}

var messageActionArrivalOrder uint64

const messageActionCensusLimit = 100

type messageActionCensus struct {
	mu      sync.Mutex
	nextSeq uint64
	entries []string
}

var eventCensus messageActionCensus
var messageCallbackMu sync.Mutex

const maxForwardSources = 1000

const (
	forwardFailureNone uint8 = iota
	forwardFailureSourceUnavailable
	forwardFailureInvalidSource
	forwardFailureInvalidDestination
	forwardFailureSendFailed
)

type forwardSource struct {
	info    types.MessageInfo
	message *waE2E.Message
}

type forwardSourceCache struct {
	mu      sync.Mutex
	entries map[string]forwardSource
	order   []string
}

var forwardedSources = forwardSourceCache{entries: make(map[string]forwardSource)}

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

func messageActionCensusAppend(entry string) {
	eventCensus.mu.Lock()
	eventCensus.nextSeq++
	entry = fmt.Sprintf("census=event seq=%d %s", eventCensus.nextSeq, entry)
	if len(eventCensus.entries) == messageActionCensusLimit {
		eventCensus.entries = eventCensus.entries[1:]
	}
	eventCensus.entries = append(eventCensus.entries, entry)
	eventCensus.mu.Unlock()
	messageActionDiagnostic("%s", entry)
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

func (diagnostics *rawPresenceDiagnostics) reset(enabled bool) {
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	diagnostics.total = 0
	diagnostics.entries = nil
	diagnostics.enabled.Store(enabled)
}

func (diagnostics *rawPresenceDiagnostics) record(event *events.Presence) uint64 {
	if !diagnostics.enabled.Load() {
		return 0
	}
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	if !diagnostics.enabled.Load() {
		return 0
	}
	diagnostics.total++
	if len(diagnostics.entries) == maxRawPresenceDiagnosticEntries {
		copy(diagnostics.entries, diagnostics.entries[1:])
		diagnostics.entries = diagnostics.entries[:maxRawPresenceDiagnosticEntries-1]
	}
	diagnostics.entries = append(diagnostics.entries, rawPresenceDiagnosticEntry{
		sequence:        diagnostics.total,
		server:          classifyPresenceServer(event.From.Server),
		unavailable:     event.Unavailable,
		lastSeenPresent: !event.LastSeen.IsZero(),
	})
	return diagnostics.total
}

func (diagnostics *rawPresenceDiagnostics) update(sequence uint64, normalized, normalization, dispatch string) {
	if !diagnostics.enabled.Load() {
		return
	}
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	for index := range diagnostics.entries {
		if diagnostics.entries[index].sequence == sequence {
			diagnostics.entries[index].normalized = normalized
			diagnostics.entries[index].normalization = normalization
			diagnostics.entries[index].dispatch = dispatch
			return
		}
	}
}

func (diagnostics *rawPresenceDiagnostics) drain() string {
	if !diagnostics.enabled.Load() {
		return ""
	}
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	if !diagnostics.enabled.Load() {
		return ""
	}
	var report strings.Builder
	fmt.Fprintf(&report, "raw presence events received: %d\n", diagnostics.total)
	firstSequence := diagnostics.total - uint64(len(diagnostics.entries)) + 1
	for index, entry := range diagnostics.entries {
		fmt.Fprintf(&report, "%d. server=%s, unavailable=%t, last_seen_present=%t, normalized=%s, normalization=%s, dispatch=%s\n", firstSequence+uint64(index), entry.server, entry.unavailable, entry.lastSeenPresent, entry.normalized, entry.normalization, entry.dispatch)
	}
	diagnostics.total = 0
	diagnostics.entries = nil
	return report.String()
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

func messageCensusKinds(msg *waE2E.Message) string {
	if msg == nil {
		return "none"
	}
	var kinds []string
	msg.ProtoReflect().Range(func(field protoreflect.FieldDescriptor, _ protoreflect.Value) bool {
		kinds = append(kinds, safeCensusName(string(field.Name())))
		return true
	})
	sort.Strings(kinds)
	if len(kinds) == 0 {
		return "none"
	}
	return strings.Join(kinds, ",")
}

func messageCensusWrappers(msg *waE2E.Message, path string) string {
	var paths []string
	for msg != nil {
		switch {
		case msg.GetDeviceSentMessage().GetMessage() != nil:
			path += ":device_sent"
			msg = msg.GetDeviceSentMessage().GetMessage()
		case msg.GetBotInvokeMessage().GetMessage() != nil:
			path += ":bot_invoke"
			msg = msg.GetBotInvokeMessage().GetMessage()
		case msg.GetEphemeralMessage().GetMessage() != nil:
			path += ":ephemeral"
			msg = msg.GetEphemeralMessage().GetMessage()
		case msg.GetViewOnceMessage().GetMessage() != nil:
			path += ":view_once"
			msg = msg.GetViewOnceMessage().GetMessage()
		case msg.GetViewOnceMessageV2().GetMessage() != nil:
			path += ":view_once_v2"
			msg = msg.GetViewOnceMessageV2().GetMessage()
		case msg.GetViewOnceMessageV2Extension().GetMessage() != nil:
			path += ":view_once_v2_extension"
			msg = msg.GetViewOnceMessageV2Extension().GetMessage()
		case msg.GetLottieStickerMessage().GetMessage() != nil:
			path += ":lottie_sticker"
			msg = msg.GetLottieStickerMessage().GetMessage()
		case msg.GetDocumentWithCaptionMessage().GetMessage() != nil:
			path += ":document_caption"
			msg = msg.GetDocumentWithCaptionMessage().GetMessage()
		case msg.GetEditedMessage().GetMessage() != nil:
			path += ":edited"
			msg = msg.GetEditedMessage().GetMessage()
		default:
			return path
		}
		paths = append(paths, path)
	}
	return strings.Join(paths, ",")
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

// Logger
type WrLogger struct{}

func (l *WrLogger) Errorf(msg string, args ...any) {
	LOG_ERROR(msg, args...)
}
func (l *WrLogger) Warnf(msg string, args ...any) {
	LOG_WARN(msg, args...)
}
func (l *WrLogger) Infof(msg string, args ...any) {
	LOG_INFO(msg, args...)
}
func (l *WrLogger) Debugf(msg string, args ...any) {
	LOG_DEBUG(msg, args...)
}

func (l *WrLogger) Sub(module string) waLog.Logger {
	return &WrLogger{}
}

// GetSelfId returns the current user's JID string for comparison (e.g. broadcast sender).
func GetSelfId(client *whatsmeow.Client) string {
	if client == nil || client.Store == nil || client.Store.ID == nil {
		return ""
	}
	return StrFromJid(*client.Store.ID)
}

// GetChatId returns the normalized chat id (conversation key): LID→PN, broadcast→per-sender, status as-is.
func GetChatId(client *whatsmeow.Client, chatJid *types.JID, senderJid *types.JID) string {
	if chatJid == nil {
		LOG_WARN("chatJid is nil")
		return ""
	}
	if chatJid.Server == types.BroadcastServer && chatJid.User == "status" {
		return StrFromJid(*chatJid)
	}
	if chatJid.Server == types.BroadcastServer && chatJid.User != "status" {
		if senderJid != nil {
			userId := GetUserId(client, nil, senderJid)
			if userId == GetSelfId(client) {
				return StrFromJid(*chatJid)
			}
			return userId
		}
	}
	if chatJid.Server == types.HiddenUserServer {
		ctx := context.Background()
		if pChatJid, _ := client.Store.LIDs.GetPNForLID(ctx, *chatJid); !pChatJid.IsEmpty() {
			return StrFromJid(pChatJid)
		}
	}
	return StrFromJid(*chatJid)
}

// GetUserId returns the normalized user/sender id: LID→PN when known; in groups use sender as-is (like nchat).
func GetUserId(client *whatsmeow.Client, chatJid *types.JID, userJid *types.JID) string {
	if userJid == nil {
		LOG_WARN("userJid is nil")
		return ""
	}
	if chatJid != nil && chatJid.Server == types.GroupServer {
		return StrFromJid(*userJid)
	}
	if userJid.Server == types.HiddenUserServer {
		ctx := context.Background()
		if pUserJid, _ := client.Store.LIDs.GetPNForLID(ctx, *userJid); !pUserJid.IsEmpty() {
			return StrFromJid(pUserJid)
		}
	}
	return StrFromJid(*userJid)
}

// Convert Jid to string without any mapping, use with care!
func StrFromJid(jid types.JID) string {
	return jid.User + "@" + jid.Server
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

// buildOrdinaryReaction validates the ordinary-message bridge contract before
// building a WhatsApp reaction. Newsletter reactions use a separate API and
// must never pass through this path.
type reactionRequest struct {
	target, destination, sender types.JID
	id                          types.MessageID
	reaction                    string
}

func newReactionRequest(target, destination, sender types.JID, id types.MessageID, reaction string) (reactionRequest, error) {
	if target.IsEmpty() || destination.IsEmpty() || sender.IsEmpty() || id == "" || reaction == "" {
		return reactionRequest{}, fmt.Errorf("reaction requires target, destination, sender, message ID, and text")
	}
	if target.Server == types.NewsletterServer || destination.Server == types.NewsletterServer {
		return reactionRequest{}, fmt.Errorf("newsletter reactions are not ordinary message reactions")
	}
	return reactionRequest{target: target, destination: destination, sender: sender, id: id, reaction: reaction}, nil
}

func buildOrdinaryReaction(client *whatsmeow.Client, target, sender types.JID, id types.MessageID, reaction string) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	request, err := newReactionRequest(target, target, sender, id, reaction)
	if err != nil {
		return nil, err
	}
	return client.BuildReaction(request.target, request.sender, request.id, request.reaction), nil
}

// buildOrdinaryEdit validates the ordinary-message bridge contract before
// building a WhatsApp edit. Newsletter messages use separate semantics and
// must never pass through this path.
func buildOrdinaryEdit(client *whatsmeow.Client, chat types.JID, id types.MessageID, replacement string) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	if chat.IsEmpty() || id == "" || strings.TrimSpace(replacement) == "" {
		return nil, fmt.Errorf("edit requires chat, message ID, and replacement text")
	}
	if chat.Server == types.NewsletterServer {
		return nil, fmt.Errorf("newsletter edits are not ordinary message edits")
	}
	return client.BuildEdit(chat, id, &waE2E.Message{Conversation: &replacement}), nil
}

func parseActionJID(raw string) (types.JID, error) {
	jid, err := types.ParseJID(raw)
	if err != nil || jid.IsEmpty() {
		return types.JID{}, fmt.Errorf("invalid JID")
	}
	return jid, nil
}

// contactDisplayName returns the display name for a contact (same order as Rust get_contact_name).
func contactDisplayName(c types.ContactInfo) string {
	if c.FullName != "" {
		return c.FullName
	}
	if c.FirstName != "" {
		return c.FirstName
	}
	if c.PushName != "" {
		return "~ " + c.PushName
	}
	if c.BusinessName != "" {
		return "+ " + c.BusinessName
	}
	return ""
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

//export C_NewClient
func C_NewClient(dbPath *C.char) {
	rawPresenceProbe.reset(os.Getenv("WPTUI_PRESENCE_DEBUG") == "1")
	requestFullHistorySync()
	goPath := C.GoString(dbPath)
	dbLog := &WrLogger{}
	container, err := sqlstore.New(context.Background(), "sqlite3", "file:"+goPath+"?_foreign_keys=on", dbLog)
	if err != nil {
		panic(err)
	}
	deviceStore, _ := container.GetFirstDevice(context.Background())
	clientLog := &WrLogger{}
	client = whatsmeow.NewClient(deviceStore, clientLog)
	configurePresenceSubscriptions(client)
}

func configurePresenceSubscriptions(client *whatsmeow.Client) {
	client.ErrorOnSubscribePresenceWithoutToken = true
}

func requestFullHistorySync() {
	// Ask WhatsApp to send the complete conversation + history list instead of
	// only a recent-activity subset. WhatsApp gates history sync requests on an
	// open phone session, and full sync must be requested before pairing
	// (DeviceProps is transmitted during the registration handshake).
	store.DeviceProps.RequireFullSync = proto.Bool(true)
	store.DeviceProps.HistorySyncConfig.FullSyncDaysLimit = proto.Uint32(365)
	store.DeviceProps.HistorySyncConfig.FullSyncSizeMbLimit = proto.Uint32(10240)
}

const viewOnceUnavailablePlaceholder = "View-once media is unavailable here. View it on your phone."

func unavailableViewOnceMessage(message *waE2E.Message) (*waE2E.Message, bool) {
	if !containsViewOnceWrapper(message) {
		return message, false
	}
	return &waE2E.Message{Conversation: proto.String(viewOnceUnavailablePlaceholder)}, true
}

func containsViewOnceWrapper(message *waE2E.Message) bool {
	for message != nil {
		switch {
		case message.GetViewOnceMessage() != nil || message.GetViewOnceMessageV2() != nil || message.GetViewOnceMessageV2Extension() != nil:
			return true
		case message.GetDeviceSentMessage().GetMessage() != nil:
			message = message.GetDeviceSentMessage().GetMessage()
		case message.GetBotInvokeMessage().GetMessage() != nil:
			message = message.GetBotInvokeMessage().GetMessage()
		case message.GetEphemeralMessage().GetMessage() != nil:
			message = message.GetEphemeralMessage().GetMessage()
		case message.GetLottieStickerMessage().GetMessage() != nil:
			message = message.GetLottieStickerMessage().GetMessage()
		case message.GetDocumentWithCaptionMessage().GetMessage() != nil:
			message = message.GetDocumentWithCaptionMessage().GetMessage()
		case message.GetEditedMessage().GetMessage() != nil:
			message = message.GetEditedMessage().GetMessage()
		default:
			return false
		}
	}
	return false
}

func ParseWebMessageInfo(selfJid types.JID, chatJid types.JID, webMsg *waWeb.WebMessageInfo) *types.MessageInfo {
	info := types.MessageInfo{
		MessageSource: types.MessageSource{
			Chat:     chatJid,
			IsFromMe: webMsg.GetKey().GetFromMe(),
			IsGroup:  chatJid.Server == types.GroupServer,
		},
		ID:        webMsg.GetKey().GetID(),
		PushName:  webMsg.GetPushName(),
		Timestamp: time.Unix(int64(webMsg.GetMessageTimestamp()), 0),
	}
	if info.IsFromMe {
		info.Sender = selfJid.ToNonAD()
	} else if webMsg.GetParticipant() != "" {
		info.Sender, _ = types.ParseJID(webMsg.GetParticipant())
	} else if webMsg.GetKey().GetParticipant() != "" {
		info.Sender, _ = types.ParseJID(webMsg.GetKey().GetParticipant())
	} else {
		info.Sender = chatJid
	}
	if info.Sender.IsEmpty() {
		return nil
	}
	return &info
}

func SliceIndex(list []string, value string, defaultValue int) int {
	index := slices.Index(list, value)
	if index == -1 {
		index = defaultValue
	}
	return index
}

func ExtensionByType(mimeType string, defaultExt string) string {
	ext := defaultExt
	exts, extErr := mime.ExtensionsByType(mimeType)
	if extErr == nil && len(exts) > 0 {
		// prefer common extensions over less common (.jpe, etc) returned by mime library
		preferredExts := []string{".jpg", ".jpeg"}
		sort.Slice(exts, func(i, j int) bool {
			return SliceIndex(preferredExts, exts[i], math.MaxInt32) < SliceIndex(preferredExts, exts[j], math.MaxInt32)
		})
		ext = exts[0]
	}

	return ext
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

func ContentToWaE2EMessage(messageType C.uint8_t, messageContent unsafe.Pointer, contextInfo *waE2E.ContextInfo) *waE2E.Message {
	switch messageType {
	case C.uint8_t(MessageTypeText):
		textMsg := (*C.TextMessage)(messageContent)
		text := C.GoString(textMsg.text)
		return &waE2E.Message{
			ExtendedTextMessage: &waE2E.ExtendedTextMessage{
				Text:        &text,
				ContextInfo: contextInfo,
			},
		}

	case C.uint8_t(MessageTypeFile):
		fileMsg := (*C.FileMessage)(messageContent)
		kind := uint8(fileMsg.kind)
		filePath := C.GoString(fileMsg.path)
		var caption *string
		if fileMsg.caption != nil {
			captionValue := C.GoString(fileMsg.caption)
			caption = &captionValue
		}
		message, err := buildFileMessage(context.Background(), kind, filePath, caption, contextInfo, client.Upload)
		if err != nil {
			panic(err)
		}
		return message

	default:
		panic(fmt.Sprintf("Unsupported message type: %d", messageType))
	}
}

type statusProtocolContext struct {
	kind string
	info *waE2E.ContextInfo
}

func statusProtocolReactionDiagnostic(info types.MessageInfo, msg *waE2E.Message) (string, bool) {
	reaction := msg.GetReactionMessage()
	if reaction == nil || !isStatusProtocolReaction(info, reaction) {
		return "", false
	}
	key := reaction.GetKey()
	return fmt.Sprintf(
		"status_protocol=reaction chat=%s sender=%s from_me=%t remote_jid=%s participant=%s key_from_me=%t id=%s emoji_codepoints=%s emoji=%s",
		info.Chat.String(), info.Sender.String(), info.IsFromMe,
		key.GetRemoteJID(), key.GetParticipant(), key.GetFromMe(), key.GetID(),
		statusProtocolEmojiCodepoints(reaction.GetText()), statusProtocolEmojiText(reaction.GetText()),
	), true
}

func isStatusProtocolReaction(info types.MessageInfo, reaction *waE2E.ReactionMessage) bool {
	key := reaction.GetKey()
	return info.Chat.String() == "status@broadcast" ||
		key.GetRemoteJID() == "status@broadcast" ||
		key.GetParticipant() == "status@broadcast"
}

func statusProtocolEmojiCodepoints(text string) string {
	if text == "" {
		return "none"
	}
	codepoints := make([]string, 0, len(text))
	for _, r := range text {
		codepoints = append(codepoints, fmt.Sprintf("U+%04X", r))
	}
	return strings.Join(codepoints, ",")
}

func statusProtocolEmojiText(text string) string {
	if text == "" {
		return "<empty>"
	}
	return text
}

func statusProtocolQuotedMessageKind(message *waE2E.Message) string {
	switch {
	case message == nil:
		return "none"
	case message.GetConversation() != "":
		return "text"
	case message.GetExtendedTextMessage() != nil:
		return "extended_text"
	case message.GetImageMessage() != nil:
		return "image"
	case message.GetVideoMessage() != nil:
		return "video"
	case message.GetAudioMessage() != nil:
		return "audio"
	case message.GetDocumentMessage() != nil:
		return "document"
	case message.GetStickerMessage() != nil:
		return "sticker"
	default:
		return "other"
	}
}

func statusProtocolContextDiagnostics(info types.MessageInfo, msg *waE2E.Message) []string {
	contexts := []statusProtocolContext{
		{kind: "extended_text", info: msg.GetExtendedTextMessage().GetContextInfo()},
		{kind: "image", info: msg.GetImageMessage().GetContextInfo()},
		{kind: "video", info: msg.GetVideoMessage().GetContextInfo()},
		{kind: "audio", info: msg.GetAudioMessage().GetContextInfo()},
		{kind: "document", info: msg.GetDocumentMessage().GetContextInfo()},
		{kind: "sticker", info: msg.GetStickerMessage().GetContextInfo()},
	}
	lines := make([]string, 0, len(contexts))
	for _, context := range contexts {
		if context.info == nil || !isStatusProtocolContext(context.info) {
			continue
		}
		quotedMessage := context.info.GetQuotedMessage()
		lines = append(lines, fmt.Sprintf(
			"status_protocol=context chat=%s sender=%s from_me=%t content=%s stanza_id=%s participant=%s remote_jid=%s poster_status_id=%s quoted_message_present=%t quoted_message_kind=%s status_source_type_present=%t status_source_type=%d status_attribution_type_present=%t status_attribution_type=%d is_group_status_present=%t is_group_status=%t",
			info.Chat.String(), info.Sender.String(), info.IsFromMe, context.kind,
			context.info.GetStanzaID(), context.info.GetParticipant(), context.info.GetRemoteJID(), context.info.GetPosterStatusID(), quotedMessage != nil, statusProtocolQuotedMessageKind(quotedMessage),
			context.info.StatusSourceType != nil, context.info.GetStatusSourceType(),
			context.info.StatusAttributionType != nil, context.info.GetStatusAttributionType(),
			context.info.IsGroupStatus != nil, context.info.GetIsGroupStatus(),
		))
	}
	return lines
}

type forwardingState struct {
	isForwarded bool
	score       uint32
}

func forwardingStateFromContext(context *waE2E.ContextInfo) forwardingState {
	if context == nil {
		return forwardingState{}
	}
	return forwardingState{isForwarded: context.GetIsForwarded(), score: context.GetForwardingScore()}
}

func forwardingStateFromMessage(message *waE2E.Message) forwardingState {
	switch {
	case message == nil:
		return forwardingState{}
	case message.GetExtendedTextMessage() != nil:
		return forwardingStateFromContext(message.GetExtendedTextMessage().GetContextInfo())
	case message.GetImageMessage() != nil:
		return forwardingStateFromContext(message.GetImageMessage().GetContextInfo())
	case message.GetVideoMessage() != nil:
		return forwardingStateFromContext(message.GetVideoMessage().GetContextInfo())
	case message.GetAudioMessage() != nil:
		return forwardingStateFromContext(message.GetAudioMessage().GetContextInfo())
	case message.GetDocumentMessage() != nil:
		return forwardingStateFromContext(message.GetDocumentMessage().GetContextInfo())
	case message.GetStickerMessage() != nil:
		return forwardingStateFromContext(message.GetStickerMessage().GetContextInfo())
	default:
		return forwardingState{}
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

func resetForwardedSourcesForTest() {
	forwardedSources.mu.Lock()
	defer forwardedSources.mu.Unlock()
	forwardedSources.entries = make(map[string]forwardSource)
	forwardedSources.order = nil
}

func removeForwardSources(chat, id string) {
	forwardedSources.mu.Lock()
	defer forwardedSources.mu.Unlock()
	for key, source := range forwardedSources.entries {
		if source.info.Chat.String() == chat && string(source.info.ID) == id {
			delete(forwardedSources.entries, key)
		}
	}
	forwardedSources.order = slices.DeleteFunc(forwardedSources.order, func(key string) bool {
		_, exists := forwardedSources.entries[key]
		return !exists
	})
}

func cacheForwardSource(info types.MessageInfo, message *waE2E.Message) {
	if message == nil || info.ID == "" || containsViewOnceWrapper(message) {
		return
	}
	key := forwardSourceKey(info.Chat, info.Sender, info.ID)
	forwardedSources.mu.Lock()
	defer forwardedSources.mu.Unlock()
	if _, exists := forwardedSources.entries[key]; !exists {
		forwardedSources.order = append(forwardedSources.order, key)
		if len(forwardedSources.order) > maxForwardSources {
			delete(forwardedSources.entries, forwardedSources.order[0])
			forwardedSources.order = forwardedSources.order[1:]
		}
	}
	forwardedSources.entries[key] = forwardSource{info: info, message: proto.Clone(message).(*waE2E.Message)}
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
		rawSource, err = proto.Marshal(msg)
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

const (
	messageActionEdit uint8 = iota
	messageActionDelete
)

type messageActionEvent struct {
	actionID, chat, sender, targetMessageID, replacement string
	occurredAt                                           int64
	arrivalOrder                                         uint64
	kind                                                 uint8
}

type messageActionProbe struct {
	present, deviceSent, botInvoke, ephemeral, viewOnce, viewOnceV2, viewOnceV2Extension, lottieSticker, documentWithCaption, edited, futureProof bool
	protocol                                                                                                                                      *waE2E.ProtocolMessage
	secretEncrypted                                                                                                                               *waE2E.SecretEncryptedMessage
	protocolPath                                                                                                                                  string
}

// messageActionProbeFromMessage follows every wrapper that the pinned
// whatsmeow Message.UnwrapRaw follows. Classification and diagnostics share it
// so the reported structure is the structure that was classified.
func messageActionProbeFromMessage(msg *waE2E.Message, path string) messageActionProbe {
	probe := messageActionProbe{present: msg != nil}
	if msg == nil {
		return probe
	}
	if protocol := msg.GetProtocolMessage(); protocol != nil {
		probe.protocol, probe.protocolPath = protocol, path+".protocol"
	}
	if secret := msg.GetSecretEncryptedMessage(); secret != nil {
		probe.secretEncrypted = secret
	}
	merge := func(child messageActionProbe) {
		probe.deviceSent = probe.deviceSent || child.deviceSent
		probe.botInvoke = probe.botInvoke || child.botInvoke
		probe.ephemeral = probe.ephemeral || child.ephemeral
		probe.viewOnce = probe.viewOnce || child.viewOnce
		probe.viewOnceV2 = probe.viewOnceV2 || child.viewOnceV2
		probe.viewOnceV2Extension = probe.viewOnceV2Extension || child.viewOnceV2Extension
		probe.lottieSticker = probe.lottieSticker || child.lottieSticker
		probe.documentWithCaption = probe.documentWithCaption || child.documentWithCaption
		probe.edited = probe.edited || child.edited
		probe.futureProof = probe.futureProof || child.futureProof
		if probe.protocol == nil && child.protocol != nil {
			probe.protocol, probe.protocolPath = child.protocol, child.protocolPath
		}
		if probe.secretEncrypted == nil && child.secretEncrypted != nil {
			probe.secretEncrypted = child.secretEncrypted
		}
	}
	child := func(message *waE2E.Message, name string, mark func()) {
		mark()
		merge(messageActionProbeFromMessage(message, path+"."+name))
	}
	if wrapper := msg.GetDeviceSentMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "device_sent", func() { probe.deviceSent = true })
	}
	if wrapper := msg.GetBotInvokeMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "bot_invoke", func() { probe.botInvoke, probe.futureProof = true, true })
	}
	if wrapper := msg.GetEphemeralMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "ephemeral", func() { probe.ephemeral, probe.futureProof = true, true })
	}
	if wrapper := msg.GetViewOnceMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "view_once", func() { probe.viewOnce, probe.futureProof = true, true })
	}
	if wrapper := msg.GetViewOnceMessageV2(); wrapper != nil {
		child(wrapper.GetMessage(), "view_once_v2", func() { probe.viewOnce, probe.viewOnceV2, probe.futureProof = true, true, true })
	}
	if wrapper := msg.GetViewOnceMessageV2Extension(); wrapper != nil {
		child(wrapper.GetMessage(), "view_once_v2_extension", func() {
			probe.viewOnce, probe.viewOnceV2, probe.viewOnceV2Extension, probe.futureProof = true, true, true, true
		})
	}
	if wrapper := msg.GetLottieStickerMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "lottie_sticker", func() { probe.lottieSticker, probe.futureProof = true, true })
	}
	if wrapper := msg.GetDocumentWithCaptionMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "document_caption", func() { probe.documentWithCaption, probe.futureProof = true, true })
	}
	if wrapper := msg.GetEditedMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "edited", func() { probe.edited, probe.futureProof = true, true })
	}
	return probe
}

func (probe messageActionProbe) hasActionProtocol() bool {
	if probe.protocol == nil {
		return false
	}
	return probe.protocol.GetType() == waE2E.ProtocolMessage_MESSAGE_EDIT || probe.protocol.GetType() == waE2E.ProtocolMessage_REVOKE
}

func (probe messageActionProbe) replacementVariant() (bool, string) {
	replacement := probe.protocol.GetEditedMessage()
	if replacement == nil {
		return false, "none"
	}
	if replacement.GetConversation() != "" {
		return true, "conversation"
	}
	if replacement.GetExtendedTextMessage().GetText() != "" {
		return true, "extended_text"
	}
	return true, "none"
}

// msgContentTypes returns a compact field-presence string for the message,
// used for diagnostics when the decrypted structure doesn't match expectations.
func msgContentTypes(msg *waE2E.Message) string {
	if msg == nil {
		return "nil"
	}
	parts := make([]string, 0, 8)
	if msg.Conversation != nil {
		parts = append(parts, "conversation")
	}
	if msg.ExtendedTextMessage != nil {
		parts = append(parts, "extended_text")
	}
	if msg.ProtocolMessage != nil {
		pm := msg.ProtocolMessage
		s := "protocol"
		if pm.GetEditedMessage() != nil {
			s += "+edited"
		}
		parts = append(parts, s)
	}
	if msg.EditedMessage != nil {
		s := "edited"
		if em := msg.EditedMessage.GetMessage(); em != nil {
			s += "+inner"
			if em.Conversation != nil {
				s += "_conv"
			}
			if em.ExtendedTextMessage != nil {
				s += "_ext"
			}
		}
		parts = append(parts, s)
	}
	if msg.ReactionMessage != nil {
		parts = append(parts, "reaction")
	}
	if len(parts) == 0 {
		return "empty"
	}
	return strings.Join(parts, ",")
}

func replacementText(msg *waE2E.Message) (string, bool) {
	if msg == nil {
		return "", false
	}
	// Direct text content — simplest case
	if replacement := msg.GetConversation(); replacement != "" {
		return replacement, true
	}
	if replacement := msg.GetExtendedTextMessage().GetText(); replacement != "" {
		return replacement, true
	}
	// ProtocolMessage wrapper: used by BuildEdit / non-secret edits
	if pm := msg.GetProtocolMessage(); pm != nil && pm.GetEditedMessage() != nil {
		return replacementText(pm.GetEditedMessage())
	}
	// EditedMessage wrapper: FutureProofMessage -> inner Message
	if em := msg.GetEditedMessage(); em != nil && em.GetMessage() != nil {
		return replacementText(em.GetMessage())
	}
	return "", false
}

type decryptSecretEncryptedMessageFunc func(context.Context, *events.Message) (*waE2E.Message, error)

func decryptSecretEncryptedMessage(ctx context.Context, evt *events.Message) (*waE2E.Message, error) {
	return client.DecryptSecretEncryptedMessage(ctx, evt)
}

func secretEditErrorClass(err error) string {
	if errors.Is(err, whatsmeow.ErrOriginalMessageSecretNotFound) {
		return "missing_original_secret"
	}
	if strings.Contains(err.Error(), "decode message protobuf") {
		return "protobuf_decode_failed"
	}
	return "decrypt_or_auth_failed"
}

func secretEditCensusResult(evt *events.Message, secret *waE2E.SecretEncryptedMessage, result, contentKind, errorClass string) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	targetID := "<missing>"
	keyPresent := secret.GetTargetMessageKey() != nil
	if secret.GetTargetMessageKey().GetID() != "" {
		targetID = messageActionIdentifier(secret.GetTargetMessageKey().GetID())
	}
	messageActionCensusAppend(fmt.Sprintf("event_type=events_message secret_edit_result=%s error_class=%s secret_enc_type=%s secret_enc_type_number=%d action_id=%s target_id=%s target_key_present=%t secret_payload_length=%d secret_iv_length=%d decrypted_content_kind=%s", result, errorClass, safeCensusName(secret.GetSecretEncType().String()), int(secret.GetSecretEncType()), messageActionIdentifier(evt.Info.ID), targetID, keyPresent, len(secret.GetEncPayload()), len(secret.GetEncIV()), contentKind))
}

// messageActionEventFromSecretEncryptedMessage handles the encrypted remote edit
// envelope before ordinary dispatch. The target key identifies the edited message;
// its participant is deliberately not used as the action sender.
func messageActionEventFromSecretEncryptedMessage(evt *events.Message, decrypt decryptSecretEncryptedMessageFunc) (messageActionEvent, bool) {
	if evt == nil {
		return messageActionEvent{}, false
	}
	probe := messageActionProbeFromMessage(evt.RawMessage, "raw")
	if probe.secretEncrypted == nil {
		probe = messageActionProbeFromMessage(evt.Message, "message")
	}
	secret := probe.secretEncrypted
	if secret == nil || secret.GetSecretEncType() != waE2E.SecretEncryptedMessage_MESSAGE_EDIT {
		return messageActionEvent{}, false
	}
	if evt.Info.ID == "" {
		secretEditCensusResult(evt, secret, "ignored", "none", "missing_action_id")
		return messageActionEvent{}, false
	}
	if secret.GetTargetMessageKey() == nil || secret.GetTargetMessageKey().GetID() == "" {
		secretEditCensusResult(evt, secret, "ignored", "none", "missing_target_key")
		return messageActionEvent{}, false
	}
	if decrypt == nil {
		secretEditCensusResult(evt, secret, "failed", "none", "decrypt_unavailable")
		return messageActionEvent{}, false
	}

	// whatsmeow decrypts evt.Message, while the envelope may be nested under a
	// wrapper. Keep the incoming event immutable and present the located envelope.
	decryptEvent := *evt
	decryptEvent.Message = &waE2E.Message{SecretEncryptedMessage: secret}
	decrypted, err := decrypt(context.Background(), &decryptEvent)
	if err != nil {
		secretEditCensusResult(evt, secret, "failed", "none", secretEditErrorClass(err))
		return messageActionEvent{}, false
	}
	replacement, ok := replacementText(decrypted)
	if !ok {
		secretEditCensusResult(evt, secret, "ignored", msgContentTypes(decrypted), "missing_replacement")
		return messageActionEvent{}, false
	}
	contentKind := "conversation"
	if decrypted.GetConversation() == "" {
		contentKind = "extended_text"
	}
	secretEditCensusResult(evt, secret, "success", contentKind, "none")
	return messageActionEvent{
		actionID:        evt.Info.ID,
		chat:            evt.Info.Chat.String(),
		sender:          evt.Info.Sender.String(),
		targetMessageID: secret.GetTargetMessageKey().GetID(),
		replacement:     replacement,
		occurredAt:      evt.Info.Timestamp.Unix(),
		arrivalOrder:    atomic.AddUint64(&messageActionArrivalOrder, 1),
		kind:            messageActionEdit,
	}, true
}

// messageActionEventFromMessage recognizes the protocol envelopes emitted by the
// pinned whatsmeow builders. Unsupported or incomplete payloads stay ordinary.
func messageActionEventFromProbe(info types.MessageInfo, probe messageActionProbe) (messageActionEvent, bool, string) {
	protocol := probe.protocol
	if protocol == nil {
		return messageActionEvent{}, false, "protocol_absent"
	}
	if info.ID == "" {
		return messageActionEvent{}, false, "missing_action_id"
	}
	if protocol.GetKey() == nil {
		return messageActionEvent{}, false, "missing_protocol_key"
	}
	if protocol.GetKey().GetID() == "" {
		return messageActionEvent{}, false, "missing_target_id"
	}

	action := messageActionEvent{
		actionID:        info.ID,
		chat:            info.Chat.String(),
		sender:          info.Sender.String(),
		targetMessageID: protocol.GetKey().GetID(),
		occurredAt:      info.Timestamp.Unix(),
		arrivalOrder:    atomic.AddUint64(&messageActionArrivalOrder, 1),
	}
	if timestampMS := protocol.GetTimestampMS(); timestampMS > 0 {
		action.occurredAt = timestampMS / 1000
	}
	if participant := protocol.GetKey().GetParticipant(); participant != "" {
		if sender, err := types.ParseJID(participant); err == nil {
			action.sender = sender.String()
		}
	}

	switch protocol.GetType() {
	case waE2E.ProtocolMessage_MESSAGE_EDIT:
		replacement, ok := replacementText(protocol.GetEditedMessage())
		if !ok {
			return messageActionEvent{}, false, "missing_replacement"
		}
		action.kind = messageActionEdit
		action.replacement = replacement
	case waE2E.ProtocolMessage_REVOKE:
		action.kind = messageActionDelete
	default:
		return messageActionEvent{}, false, "unsupported_protocol"
	}
	return action, true, ""
}

func messageActionEventFromMessage(info types.MessageInfo, msg *waE2E.Message) (messageActionEvent, bool) {
	action, ok, _ := messageActionEventFromProbe(info, messageActionProbeFromMessage(msg, "message"))
	return action, ok
}

func messageActionStructuralLine(evt *events.Message, branch, reason string) string {
	raw := messageActionProbeFromMessage(evt.RawMessage, "raw")
	normalized := messageActionProbeFromMessage(evt.Message, "message")
	source := messageActionProbe{}
	if evt.SourceWebMsg != nil {
		source = messageActionProbeFromMessage(evt.SourceWebMsg.GetMessage(), "source")
	}
	hasSignal := evt.IsEdit || raw.edited || normalized.edited || source.edited || raw.hasActionProtocol() || normalized.hasActionProtocol() || source.hasActionProtocol()
	if !hasSignal {
		return ""
	}
	selected := raw
	if !selected.hasActionProtocol() && normalized.hasActionProtocol() {
		selected = normalized
	}
	if !selected.hasActionProtocol() && source.hasActionProtocol() {
		selected = source
	}
	if selected.protocol == nil {
		selected = normalized
	}
	if selected.protocol == nil {
		selected = source
	}
	protocolType := "none"
	if selected.protocol != nil {
		protocolType = selected.protocol.GetType().String()
	}
	replacementExists, replacementVariant := selected.replacementVariant()
	return fmt.Sprintf("classifier=structural branch=%s reason=%s is_edit=%t roots=raw:%t,message:%t,source:%t wrappers=edited:%t,ephemeral:%t,view_once:%t,view_once_v2:%t,view_once_v2_extension:%t,device_sent:%t,document_caption:%t,bot_invoke:%t,lottie_sticker:%t,future_proof:%t protocol_path=%s protocol_type=%s protocol_key=%t protocol_key_id=%t replacement_exists=%t replacement_variant=%s action_id=%s target_id=%s chat=%s sender=%s", branch, reason, evt.IsEdit, raw.present, normalized.present, source.present, raw.edited || normalized.edited || source.edited, raw.ephemeral || normalized.ephemeral || source.ephemeral, raw.viewOnce || normalized.viewOnce || source.viewOnce, raw.viewOnceV2 || normalized.viewOnceV2 || source.viewOnceV2, raw.viewOnceV2Extension || normalized.viewOnceV2Extension || source.viewOnceV2Extension, raw.deviceSent || normalized.deviceSent || source.deviceSent, raw.documentWithCaption || normalized.documentWithCaption || source.documentWithCaption, raw.botInvoke || normalized.botInvoke || source.botInvoke, raw.lottieSticker || normalized.lottieSticker || source.lottieSticker, raw.futureProof || normalized.futureProof || source.futureProof, selected.protocolPath, protocolType, selected.protocol.GetKey() != nil, selected.protocol.GetKey().GetID() != "", replacementExists, replacementVariant, messageActionIdentifier(evt.Info.ID), messageActionIdentifier(selected.protocol.GetKey().GetID()), messageActionIdentifier(evt.Info.Chat.String()), messageActionIdentifier(evt.Info.Sender.String()))
}

func messageActionStructuralDiagnostic(evt *events.Message, branch, reason string) {
	if line := messageActionStructuralLine(evt, branch, reason); line != "" {
		messageActionDiagnostic("%s", line)
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

func dispatchMessageActionEvent(action messageActionEvent) {
	if eventHandler.callback == nil {
		return
	}
	cactionID := C.CString(action.actionID)
	cchat := C.CString(action.chat)
	csender := C.CString(action.sender)
	target := C.CString(action.targetMessageID)
	replacement := C.CString(action.replacement)
	defer C.free(unsafe.Pointer(cactionID))
	defer C.free(unsafe.Pointer(cchat))
	defer C.free(unsafe.Pointer(csender))
	defer C.free(unsafe.Pointer(target))
	defer C.free(unsafe.Pointer(replacement))

	payload := (*C.MessageActionEvent)(C.malloc(C.sizeof_MessageActionEvent))
	if payload == nil {
		return
	}
	payload.actionID = cactionID
	payload.chat = cchat
	payload.sender = csender
	payload.targetMessageID = target
	payload.replacement = replacement
	payload.occurredAt = C.int64_t(action.occurredAt)
	payload.arrivalOrder = C.uint64_t(action.arrivalOrder)
	payload.kind = C.uint8_t(action.kind)
	C.callEventCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeMessageAction), data: unsafe.Pointer(payload)})
	C.free(unsafe.Pointer(payload))
}

// dispatchIncomingMessage keeps action envelopes out of the ordinary message
// path, whose insertion logic would otherwise treat an edit body as a base message.
func dispatchIncomingMessage(
	evt *events.Message,
	dispatchAction func(messageActionEvent),
	dispatchMessage func(types.MessageInfo, *waE2E.Message, bool),
) {
	dispatchIncomingMessageWithDecrypt(evt, dispatchAction, dispatchMessage, decryptSecretEncryptedMessage)
}

func dispatchIncomingMessageWithDecrypt(
	evt *events.Message,
	dispatchAction func(messageActionEvent),
	dispatchMessage func(types.MessageInfo, *waE2E.Message, bool),
	decrypt decryptSecretEncryptedMessageFunc,
) {
	if action, ok := messageActionEventFromSecretEncryptedMessage(evt, decrypt); ok {
		dispatchAction(action)
		return
	}
	if action, ok := messageActionEventFromIncomingMessage(evt); ok {
		dispatchAction(action)
		return
	}
	dispatchMessage(evt.Info, evt.Message, false)
}

type reactionEvent struct {
	chat, targetMessageID, participant, text string
	isFromMe                                 bool
}

func dispatchReactionEvent(reaction reactionEvent) {
	if eventHandler.callback == nil {
		return
	}

	cchat := C.CString(reaction.chat)
	ctarget := C.CString(reaction.targetMessageID)
	cparticipant := C.CString(reaction.participant)
	ctext := C.CString(reaction.text)
	defer C.free(unsafe.Pointer(cchat))
	defer C.free(unsafe.Pointer(ctarget))
	defer C.free(unsafe.Pointer(cparticipant))
	defer C.free(unsafe.Pointer(ctext))

	payload := (*C.ReactionEvent)(C.malloc(C.sizeof_ReactionEvent))
	if payload == nil {
		return
	}
	payload.chat = cchat
	payload.targetMessageID = ctarget
	payload.participant = cparticipant
	payload.text = ctext
	payload.isFromMe = C.bool(reaction.isFromMe)

	C.callEventCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeReaction), data: unsafe.Pointer(payload)})
	C.free(unsafe.Pointer(payload))
}

func reactionEventFromMessage(info types.MessageInfo, msg *waE2E.Message) (reactionEvent, bool) {
	reaction := msg.GetReactionMessage()
	if reaction == nil || reaction.GetKey() == nil || reaction.GetKey().GetID() == "" {
		return reactionEvent{}, false
	}
	return reactionEvent{chat: info.Chat.String(), targetMessageID: reaction.GetKey().GetID(), participant: info.Sender.String(), text: reaction.GetText(), isFromMe: info.IsFromMe}, true
}

//export C_DownloadFile
func C_DownloadFile(fileId *C.char, basePath *C.char) C.uint8_t {
	goFileId := C.GoString(fileId)
	goBasePath := C.GoString(basePath)
	status := DownloadFromFileId(client, goFileId, goBasePath)
	return C.uint8_t(status)
}

func AddEventHandlers() {
	client.AddEventHandler(func(rawEvt any) {
		messageActionCensusDiagnostic(rawEvt)
		switch evt := rawEvt.(type) {
		case *events.Connected:
			handleConnected(
				client.SendPresence,
				func() {
					C.callEventCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeConnected), data: nil})
				},
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
			LOG_INFO("AppStateSyncComplete %v", evt)
			if evt.Name == appstate.WAPatchRegular {
				LOG_INFO("AppStateSyncComplete (WAPatchRegular) %v", evt)

				cevent := C.Event{
					kind: C.uint8_t(EventTypeAppStateSyncComplete),
					data: nil,
				}
				C.callEventCallback(eventHandler, &cevent)
			}

		case *events.Message:
			dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage)

		case *events.Receipt:

			receiptKind := -1
			if evt.Type == types.ReceiptTypeRead || evt.Type == types.ReceiptTypeReadSelf {
				receiptKind = 0
			}

			if receiptKind != -1 {
				LOG_DEBUG("%#v was read by %s at %s", evt.MessageIDs, evt.SourceString(), evt.Timestamp)
				n := len(evt.MessageIDs)
				cmessageIds := (**C.char)(C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(uintptr(0)))))
				messageIds := unsafe.Slice(cmessageIds, len(evt.MessageIDs))
				for i, id := range evt.MessageIDs {
					messageIds[i] = C.CString(id)
				}

				cchatId := jidToC(evt.MessageSource.Chat)

				creceipt := (*C.ReceiptEvent)(C.malloc(C.sizeof_ReceiptEvent))
				creceipt.kind = C.uint8_t(EventTypeReceipt)
				creceipt.id = cchatId
				creceipt.messageIDs = cmessageIds
				creceipt.size = C.size_t(n)

				cevent := C.Event{
					kind: C.uint8_t(EventTypeReceipt),
					data: unsafe.Pointer(creceipt),
				}
				C.callEventCallback(eventHandler, &cevent)
			}

		case *events.HistorySync:
			percent := evt.Data.GetProgress()
			LOG_INFO(
				"History sync: type=%s chunk=%d progress=%d conversations=%d messages=%d",
				evt.Data.GetSyncType().String(),
				evt.Data.GetChunkOrder(),
				percent,
				len(evt.Data.GetConversations()),
				countSyncMessages(evt.Data.GetConversations()),
			)
			cpercent := (*C.uint8_t)(C.malloc(C.size_t(unsafe.Sizeof(uint8(0)))))
			*cpercent = C.uint8_t(percent)

			cevent := C.Event{
				kind: C.uint8_t(EventTypeSyncProgress),
				data: unsafe.Pointer(cpercent),
			}

			C.callEventCallback(eventHandler, &cevent)

			C.free(unsafe.Pointer(cpercent))

			conversations := evt.Data.GetConversations()
			// The default history downloader may store secrets asynchronously. Store
			// them synchronously here before parsing encrypted history edits.
			client.DangerousInternals().StoreHistoricalMessageSecrets(context.Background(), conversations)
			for _, conversation := range conversations {
				chatJid, err := types.ParseJID(conversation.GetID())
				if err != nil {
					LOG_WARN("history message ignored source=history_sync reason=invalid_chat")
					continue
				}

				// Register the chat itself even when the sync batch carries no
				// messages (older/archived/muted conversations). Previously only
				// conversations that shipped at least one message became chats,
				// which left the chat list stuck at the messages-only subset.
				var lastMsgTime int64
				if ts := conversation.GetLastMsgTimestamp(); ts != 0 {
					lastMsgTime = int64(ts)
				}
				chatEvent := (*C.ChatEvent)(C.malloc(C.sizeof_ChatEvent))
				chatEvent.chat = jidToC(chatJid)
				chatEvent.lastMessageTime = C.int64_t(lastMsgTime)
				cevent := C.Event{
					kind: C.uint8_t(EventTypeChat),
					data: unsafe.Pointer(chatEvent),
				}
				C.callEventCallback(eventHandler, &cevent)

				syncMessages := conversation.GetMessages()

				for _, syncMessage := range syncMessages {
					webMessageInfo := syncMessage.Message
					if webMessageInfo == nil {
						continue
					}
					parsed, err := client.ParseWebMessage(chatJid, webMessageInfo)
					if err != nil {
						LOG_WARN("history message ignored source=history_sync reason=parse_failed")
						continue
					}
					dispatchIncomingMessage(parsed, dispatchMessageActionEvent, func(info types.MessageInfo, message *waE2E.Message, _ bool) {
						HandleMessage(info, message, true)
					})
				}
			}
		}
	})
}

func countSyncMessages(conversations []*waHistorySync.Conversation) int {
	total := 0
	for _, conversation := range conversations {
		total += len(conversation.GetMessages())
	}
	return total
}

type sendPresenceFunc func(context.Context, types.Presence) error
type connectedReadyFunc func()
type presenceWarningFunc func(string, ...any)

type lidToPNFunc func(context.Context, types.JID) (types.JID, error)

func normalizePresenceJID(ctx context.Context, jid types.JID, getPNForLID lidToPNFunc) (types.JID, string) {
	if jid.Server != types.HiddenUserServer {
		return jid, "not-needed"
	}
	pn, err := getPNForLID(ctx, jid)
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) || errors.Is(ctx.Err(), context.DeadlineExceeded) {
			return jid, "timeout"
		}
		return jid, "error"
	}
	if pn.IsEmpty() {
		return jid, "missing"
	}
	return pn, "ok"
}

func dispatchPresenceEvent(event *events.Presence, getPNForLID lidToPNFunc, record func(*events.Presence) uint64, update func(uint64, string, string, string), dispatch func(string, bool, int64)) {
	sequence := record(event)
	ctx, cancel := context.WithTimeout(context.Background(), presenceNormalizationTimeout)
	defer cancel()
	from, normalization := normalizePresenceJID(ctx, event.From, getPNForLID)
	normalized := "fallback-lid"
	if from.Server == types.DefaultUserServer {
		normalized = "pn"
	}
	lastSeen := int64(0)
	if !event.LastSeen.IsZero() {
		lastSeen = event.LastSeen.Unix()
	}
	dispatch(from.ToNonAD().String(), event.Unavailable, lastSeen)
	update(sequence, normalized, normalization, "called")
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

func handleConnected(sendPresence sendPresenceFunc, connected connectedReadyFunc, warn presenceWarningFunc) bool {
	if err := sendPresence(context.Background(), types.PresenceAvailable); err != nil {
		warn("Failed to announce presence after connection; presence subscriptions will wait for the next connection: %v", err)
		return false
	}
	connected()
	return true
}

//export C_Connect
func C_Connect(handler C.QrCallback, data unsafe.Pointer) {
	AddEventHandlers()
	if client.Store.ID == nil {
		LOG_INFO(
			"Pairing: DeviceProps require_full_sync=%t days_limit=%d size_mb=%d platform=%s",
			store.DeviceProps.GetRequireFullSync(),
			store.DeviceProps.GetHistorySyncConfig().GetFullSyncDaysLimit(),
			store.DeviceProps.GetHistorySyncConfig().GetFullSyncSizeMbLimit(),
			store.DeviceProps.GetPlatformType(),
		)
		qrChan, _ = client.GetQRChannel(context.Background())
		err := client.Connect()
		if err != nil {
			panic(err)
		}

		for evt := range qrChan {
			if evt.Event == "code" {
				code := C.CString(evt.Code)
				defer C.free(unsafe.Pointer(code))
				C.callQrCallback(handler, code, data)
			}
		}
	} else {
		err := client.Connect()
		if err != nil {
			panic(err)
		}
	}

}

const (
	subscribePresenceAccepted uint8 = iota
	subscribePresenceNoPrivacyToken
	subscribePresenceRejected
)

//export C_SubscribePresence
func C_SubscribePresence(cjid C.JID) C.uint8_t {
	jid := cToJid(cjid)
	if client == nil {
		return C.uint8_t(subscribePresenceRejected)
	}
	result := subscribePresence(jid, client.SubscribePresence)
	if result == subscribePresenceNoPrivacyToken {
		LOG_WARN("Failed to subscribe to presence: no privacy token")
	} else if result == subscribePresenceRejected {
		LOG_WARN("Failed to subscribe to presence")
	}
	return C.uint8_t(result)
}

type subscribePresenceFunc func(context.Context, types.JID) error

func subscribePresence(jid types.JID, subscribe subscribePresenceFunc) uint8 {
	if jid.Server != types.DefaultUserServer {
		return subscribePresenceRejected
	}
	if err := subscribe(context.Background(), jid); err != nil {
		if errors.Is(err, whatsmeow.ErrNoPrivacyToken) {
			return subscribePresenceNoPrivacyToken
		}
		return subscribePresenceRejected
	}
	return subscribePresenceAccepted
}

//export C_PairPhone
func C_PairPhone(phone *C.char) *C.char {
	goPhone := C.GoString(phone)
	code, err := client.PairPhone(context.Background(), goPhone, true, whatsmeow.PairClientChrome, "Chrome (Linux)")
	if err != nil {
		panic(err)
	}
	cCode := C.CString(code)
	return cCode
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
		return quotedTextMessage(C.GoString((*C.TextMessage)(messageContent).text))
	case C.uint8_t(MessageTypeFile):
		file := (*C.FileMessage)(messageContent)
		caption := ""
		if file.caption != nil {
			caption = C.GoString(file.caption)
		}
		return quotedFileMessage(uint8(file.kind), caption)
	default:
		return nil
	}
}

type forwardRequest struct {
	sourceChat, sourceSender types.JID
	sourceID                 types.MessageID
	destinations             []types.JID
}

func forwardableJID(jid types.JID) bool {
	return !jid.IsEmpty() && jid.Server != types.BroadcastServer && jid.Server != types.NewsletterServer
}

func newForwardRequest(sourceChat, sourceSender, sourceID string, destinations []string) (forwardRequest, error) {
	chat, err := parseActionJID(sourceChat)
	if err != nil || !forwardableJID(chat) {
		return forwardRequest{}, fmt.Errorf("forward source chat is invalid")
	}
	sender, err := parseActionJID(sourceSender)
	if err != nil || sender.IsEmpty() || sourceID == "" {
		return forwardRequest{}, fmt.Errorf("forward requires source sender and message ID")
	}
	if len(destinations) == 0 {
		return forwardRequest{}, fmt.Errorf("forward requires at least one destination")
	}
	request := forwardRequest{sourceChat: chat, sourceSender: sender, sourceID: types.MessageID(sourceID)}
	for _, raw := range destinations {
		destination, err := parseActionJID(raw)
		if err != nil || !forwardableJID(destination) {
			return forwardRequest{}, fmt.Errorf("forward destination is invalid")
		}
		request.destinations = append(request.destinations, destination)
	}
	return request, nil
}

func forwardingContext(existing *waE2E.ContextInfo, sourceIsFromMe bool) *waE2E.ContextInfo {
	context := &waE2E.ContextInfo{}
	if existing != nil {
		context = proto.Clone(existing).(*waE2E.ContextInfo)
	}
	if sourceIsFromMe {
		context.IsForwarded = proto.Bool(false)
		context.ForwardingScore = nil
		return context
	}
	forwarded := true
	score := context.GetForwardingScore() + 1
	if score > 5 {
		score = 5
	}
	context.ForwardingScore = &score
	context.IsForwarded = &forwarded
	return context
}

func sourceOwnedByCurrentUser(sourceIsFromMe bool, sourceSender, self types.JID) bool {
	return sourceIsFromMe || sourceSender.ToNonAD() == self.ToNonAD()
}

func forwardSourceFromBytes(raw []byte) (*waE2E.Message, uint8) {
	if len(raw) == 0 {
		return nil, forwardFailureSourceUnavailable
	}
	message := &waE2E.Message{}
	if err := proto.Unmarshal(raw, message); err != nil {
		return nil, forwardFailureSourceUnavailable
	}
	return message, forwardFailureNone
}

func prepareForwardMessage(source *waE2E.Message, sourceIsFromMe bool) (*waE2E.Message, error) {
	if source == nil {
		return nil, fmt.Errorf("forward source is unavailable")
	}
	forwarded, ok := proto.Clone(source).(*waE2E.Message)
	if !ok {
		return nil, fmt.Errorf("forward source cannot be cloned")
	}
	switch {
	case forwarded.Conversation != nil:
		forwarded.ExtendedTextMessage = &waE2E.ExtendedTextMessage{Text: forwarded.Conversation, ContextInfo: forwardingContext(nil, sourceIsFromMe)}
		forwarded.Conversation = nil
	case forwarded.ExtendedTextMessage != nil:
		forwarded.ExtendedTextMessage.ContextInfo = forwardingContext(forwarded.ExtendedTextMessage.ContextInfo, sourceIsFromMe)
	case forwarded.ImageMessage != nil:
		forwarded.ImageMessage.ContextInfo = forwardingContext(forwarded.ImageMessage.ContextInfo, sourceIsFromMe)
	case forwarded.VideoMessage != nil:
		forwarded.VideoMessage.ContextInfo = forwardingContext(forwarded.VideoMessage.ContextInfo, sourceIsFromMe)
	case forwarded.AudioMessage != nil:
		forwarded.AudioMessage.ContextInfo = forwardingContext(forwarded.AudioMessage.ContextInfo, sourceIsFromMe)
	case forwarded.DocumentMessage != nil:
		forwarded.DocumentMessage.ContextInfo = forwardingContext(forwarded.DocumentMessage.ContextInfo, sourceIsFromMe)
	case forwarded.StickerMessage != nil:
		forwarded.StickerMessage.ContextInfo = forwardingContext(forwarded.StickerMessage.ContextInfo, sourceIsFromMe)
	default:
		return nil, fmt.Errorf("message content is not forwardable")
	}
	return forwarded, nil
}

//export C_ForwardMessage
func C_ForwardMessage(sourceID *C.char, sourceChat C.JID, sourceSender C.JID, sourceIsFromMe C.bool, destinations **C.char, destinationCount C.size_t, forwardSource *C.uint8_t, forwardSourceLen C.size_t) C.ForwardResult {
	result := C.ForwardResult{}
	if sourceID == nil || sourceChat == nil || sourceSender == nil || destinations == nil || destinationCount == 0 {
		return result
	}
	rawDestinations := unsafe.Slice(destinations, int(destinationCount))
	destinationStrings := make([]string, 0, len(rawDestinations))
	for _, destination := range rawDestinations {
		if destination == nil {
			return C.ForwardResult{failed: C.uint32_t(destinationCount)}
		}
		destinationStrings = append(destinationStrings, C.GoString(destination))
	}
	request, err := newForwardRequest(C.GoString(sourceChat), C.GoString(sourceSender), C.GoString(sourceID), destinationStrings)
	if err != nil {
		return C.ForwardResult{failed: C.uint32_t(destinationCount), failure: C.uint8_t(forwardFailureInvalidSource)}
	}
	if client == nil || client.Store == nil || client.Store.ID == nil {
		return C.ForwardResult{failed: C.uint32_t(destinationCount), failure: C.uint8_t(forwardFailureSendFailed)}
	}
	if forwardSource == nil || forwardSourceLen == 0 {
		return C.ForwardResult{failed: C.uint32_t(destinationCount), failure: C.uint8_t(forwardFailureSourceUnavailable)}
	}
	sourceMessage, failure := forwardSourceFromBytes(unsafe.Slice((*byte)(unsafe.Pointer(forwardSource)), int(forwardSourceLen)))
	if failure != forwardFailureNone {
		return C.ForwardResult{failed: C.uint32_t(destinationCount), failure: C.uint8_t(failure)}
	}
	sourceOwned := sourceOwnedByCurrentUser(bool(sourceIsFromMe), request.sourceSender, *client.Store.ID)
	for _, destination := range request.destinations {
		message, err := prepareForwardMessage(sourceMessage, sourceOwned)
		if err != nil {
			result.failed++
			continue
		}
		response, err := client.SendMessage(context.Background(), destination, message)
		if err != nil {
			LOG_WARN("forward send failed: %v", err)
			result.failed++
			continue
		}
		result.succeeded++
		HandleMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: destination, Sender: *client.Store.ID, IsFromMe: true}, ID: response.ID, Timestamp: response.Timestamp}, message, false)
	}
	return result
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
	} else {
		var messageInfo types.MessageInfo
		messageInfo.Chat = jid
		messageInfo.IsFromMe = true
		messageInfo.Sender = *client.Store.ID

		messageInfo.ID = sendResponse.ID
		messageInfo.Timestamp = sendResponse.Timestamp

		LOG_INFO("Message sent: %s %s", messageInfo.ID, messageInfo.Chat)
		HandleMessage(messageInfo, message, false)
	}
}

//export C_ReactToMessage
func C_ReactToMessage(targetJID C.JID, destinationJID C.JID, senderJID C.JID, messageID *C.char, reaction *C.char) C.uint8_t {
	if targetJID == nil || destinationJID == nil || senderJID == nil || messageID == nil || reaction == nil {
		return 1
	}
	target, err := parseActionJID(C.GoString(targetJID))
	if err != nil {
		return 1
	}
	destination, err := parseActionJID(C.GoString(destinationJID))
	if err != nil {
		return 1
	}
	sender, err := parseActionJID(C.GoString(senderJID))
	if err != nil {
		return 1
	}
	request, err := newReactionRequest(target, destination, sender, types.MessageID(C.GoString(messageID)), C.GoString(reaction))
	if err != nil {
		LOG_WARN("reaction rejected: %v", err)
		return 1
	}
	message, err := buildOrdinaryReaction(client, request.target, request.sender, request.id, request.reaction)
	if err != nil {
		LOG_WARN("reaction rejected: %v", err)
		return 1
	}
	if _, err := client.SendMessage(context.Background(), request.destination, message); err != nil {
		LOG_WARN("reaction send failed: %v", err)
		return 1
	}
	HandleMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: request.destination, Sender: *client.Store.ID, IsFromMe: true}}, message, false)
	return 0
}

//export C_EditMessage
func C_EditMessage(chatJID C.JID, messageID *C.char, replacement *C.char) C.uint8_t {
	if chatJID == nil || messageID == nil || replacement == nil {
		return 1
	}
	chat, err := parseActionJID(C.GoString(chatJID))
	if err != nil {
		return 1
	}
	message, err := buildOrdinaryEdit(client, chat, types.MessageID(C.GoString(messageID)), C.GoString(replacement))
	if err != nil {
		LOG_WARN("edit rejected: %v", err)
		return 1
	}
	if _, err := client.SendMessage(context.Background(), chat, message); err != nil {
		LOG_WARN("edit send failed: %v", err)
		return 1
	}
	return 0
}

// buildOrdinaryRevoke validates the ordinary-message revoke contract before
// building a WhatsApp revoke. Newsletter revocations use a separate API and
// must never pass through this path.
func buildOrdinaryRevoke(client *whatsmeow.Client, chat, sender types.JID, id types.MessageID) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	if chat.IsEmpty() || sender.IsEmpty() || id == "" {
		return nil, fmt.Errorf("revoke requires chat, sender, and message ID")
	}
	if chat.Server == types.NewsletterServer {
		return nil, fmt.Errorf("newsletter revocations are not ordinary message revocations")
	}
	return client.BuildRevoke(chat, sender, id), nil
}

//export C_RevokeMessage
func C_RevokeMessage(chatJID C.JID, senderJID C.JID, messageID *C.char) C.uint8_t {
	if chatJID == nil || senderJID == nil || messageID == nil {
		return 1
	}
	chat, err := parseActionJID(C.GoString(chatJID))
	if err != nil {
		return 1
	}
	sender, err := parseActionJID(C.GoString(senderJID))
	if err != nil {
		return 1
	}
	id := types.MessageID(C.GoString(messageID))
	message, err := buildOrdinaryRevoke(client, chat, sender, id)
	if err != nil {
		LOG_WARN("revoke rejected: %v", err)
		return 1
	}
	if _, err := client.SendMessage(context.Background(), chat, message); err != nil {
		LOG_WARN("revoke send failed: %v", err)
		return 1
	}
	removeForwardSources(chat.String(), string(id))
	return 0
}

// TODO: Free the memory allocated for C.JID and C.Contact

//export C_GetContacts
func C_GetContacts() C.GetContactsResult {
	ctx := context.Background()
	var entries []C.ContactEntry

	// Contacts (with LID aliases so group senders keyed by LID resolve to a name).
	contacts, err := client.Store.Contacts.GetAllContacts(ctx)
	if err != nil {
		panic(err)
	}
	for jid, contact := range contacts {
		name := contactDisplayName(contact)
		if name == "" {
			continue
		}
		cName := C.CString(name)
		entries = append(entries, C.ContactEntry{jid: jidToC(jid), name: cName})
		if jid.Server != types.HiddenUserServer {
			if lid, _ := client.Store.LIDs.GetLIDForPN(ctx, jid); !lid.IsEmpty() {
				entries = append(entries, C.ContactEntry{jid: jidToC(lid), name: cName})
			}
		}
	}

	// Groups.
	groups, err := client.GetJoinedGroups(ctx)
	if err != nil {
		panic(err)
	}
	for _, group := range groups {
		entries = append(entries, C.ContactEntry{
			jid:  jidToC(group.JID),
			name: C.CString(group.GroupName.Name),
		})
	}

	n := len(entries)
	c_entries := C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(C.ContactEntry{})))
	entryList := unsafe.Slice((*C.ContactEntry)(c_entries), n)
	for i := range n {
		entryList[i] = entries[i]
	}

	return C.GetContactsResult{
		entries: (*C.ContactEntry)(c_entries),
		size:    C.uint32_t(n),
	}
}

const (
	profilePictureStatusAvailable uint8 = iota
	profilePictureStatusUnavailable
	profilePictureStatusInvalidJID
	profilePictureStatusClientUnavailable
	profilePictureStatusCancelled
	profilePictureStatusMetadataFailed
	profilePictureStatusEmptyURL
	profilePictureStatusDownloadFailed
	profilePictureStatusOversized
	profilePictureStatusInvalidImage
)

const (
	profilePictureMaxSize int64 = 512 * 1024
	profilePictureTimeout       = 8 * time.Second
)

var errProfilePictureOversized = errors.New("profile picture exceeds size limit")

type profilePictureLookup func(
	context.Context,
	types.JID,
	*whatsmeow.GetProfilePictureParams,
) (*types.ProfilePictureInfo, error)

type profilePictureDownload func(context.Context, string, int64) ([]byte, error)

type profilePictureOutcome struct {
	status      uint8
	pictureID   string
	pictureType string
	data        []byte
}

func fetchProfilePicture(ctx context.Context, jidText string, lookup profilePictureLookup, download profilePictureDownload) profilePictureOutcome {
	jidText = strings.TrimSpace(jidText)
	jid, err := types.ParseJID(jidText)
	if err != nil || strings.Count(jidText, "@") != 1 || jid.User == "" || (jid.Server != types.DefaultUserServer && jid.Server != types.HiddenUserServer && jid.Server != types.GroupServer) {
		return profilePictureOutcome{status: profilePictureStatusInvalidJID}
	}
	if lookup == nil || download == nil {
		return profilePictureOutcome{status: profilePictureStatusClientUnavailable}
	}

	info, err := lookup(ctx, jid.ToNonAD(), &whatsmeow.GetProfilePictureParams{Preview: true})
	if errors.Is(err, whatsmeow.ErrProfilePictureUnauthorized) || errors.Is(err, whatsmeow.ErrProfilePictureNotSet) {
		return profilePictureOutcome{status: profilePictureStatusUnavailable}
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) || ctx.Err() != nil {
		return profilePictureOutcome{status: profilePictureStatusCancelled}
	}
	if err != nil || info == nil {
		return profilePictureOutcome{status: profilePictureStatusMetadataFailed}
	}
	if info.URL == "" {
		return profilePictureOutcome{status: profilePictureStatusEmptyURL, pictureID: info.ID, pictureType: info.Type}
	}

	data, err := download(ctx, info.URL, profilePictureMaxSize)
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) || ctx.Err() != nil {
		return profilePictureOutcome{status: profilePictureStatusCancelled, pictureID: info.ID, pictureType: info.Type}
	}
	if errors.Is(err, errProfilePictureOversized) {
		return profilePictureOutcome{status: profilePictureStatusOversized, pictureID: info.ID, pictureType: info.Type}
	}
	if err != nil {
		return profilePictureOutcome{status: profilePictureStatusDownloadFailed, pictureID: info.ID, pictureType: info.Type}
	}
	if int64(len(data)) > profilePictureMaxSize {
		return profilePictureOutcome{status: profilePictureStatusOversized, pictureID: info.ID, pictureType: info.Type}
	}
	if len(data) == 0 {
		return profilePictureOutcome{status: profilePictureStatusInvalidImage, pictureID: info.ID, pictureType: info.Type}
	}
	if _, _, err := image.DecodeConfig(bytes.NewReader(data)); err != nil {
		return profilePictureOutcome{status: profilePictureStatusInvalidImage, pictureID: info.ID, pictureType: info.Type}
	}
	return profilePictureOutcome{status: profilePictureStatusAvailable, pictureID: info.ID, pictureType: info.Type, data: data}
}

func downloadProfilePicture(ctx context.Context, url string, limit int64) ([]byte, error) {
	response, err := client.DangerousInternals().DoMediaDownloadRequest(ctx, url)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, limit+1))
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > limit {
		return nil, errProfilePictureOversized
	}
	return data, nil
}

func profilePictureToC(outcome profilePictureOutcome) C.ProfilePictureResult {
	result := C.ProfilePictureResult{status: C.uint8_t(outcome.status)}
	if outcome.pictureID != "" {
		result.picture_id = C.CString(outcome.pictureID)
	}
	if outcome.pictureType != "" {
		result.picture_type = C.CString(outcome.pictureType)
	}
	if len(outcome.data) > 0 {
		result.data = (*C.uint8_t)(C.CBytes(outcome.data))
		result.size = C.uint32_t(len(outcome.data))
	}
	return result
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

	settings, err := client.Store.ChatSettings.GetChatSettings(ctx, jid)
	if err != nil {
		LOG_WARN("failed to get chat settings for %s: %v", jid, err)
		return C.ChatSettings{}
	}

	if !settings.Found {
		if jid.Server == types.DefaultUserServer {
			// App state often stores settings under LID JID, try that if PN lookup missed.
			if lidJID, mapErr := client.Store.LIDs.GetLIDForPN(ctx, jid); mapErr == nil && !lidJID.IsEmpty() {
				if altSettings, altErr := client.Store.ChatSettings.GetChatSettings(ctx, lidJID.ToNonAD()); altErr == nil && altSettings.Found {
					settings = altSettings
				}
			}
		} else if jid.Server == types.HiddenUserServer {
			if pnJID, mapErr := client.Store.LIDs.GetPNForLID(ctx, jid); mapErr == nil && !pnJID.IsEmpty() {
				if altSettings, altErr := client.Store.ChatSettings.GetChatSettings(ctx, pnJID.ToNonAD()); altErr == nil && altSettings.Found {
					settings = altSettings
				}
			}
		}
	}

	mutedUntil := int64(0)
	if !settings.MutedUntil.IsZero() {
		mutedUntil = settings.MutedUntil.Unix()
	}

	return C.ChatSettings{
		found:       C.bool(settings.Found),
		muted_until: C.int64_t(mutedUntil),
		pinned:      C.bool(settings.Pinned),
		archived:    C.bool(settings.Archived),
	}
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

//export C_Disconnect
func C_Disconnect() {
	client.Disconnect()
}

const (
	// logoutStatusLoggedOut: remote revocation succeeded and the local store
	// was cleared (the linked device is gone from the phone too).
	logoutStatusLoggedOut uint8 = 0
	// logoutStatusNotLoggedIn: there is no paired session to remove.
	logoutStatusNotLoggedIn uint8 = 1
	// logoutStatusFailed: the remote logout request was rejected/failed and
	// even the local fallback could not clear the store.
	logoutStatusFailed uint8 = 2
	// logoutStatusLocalOnly: remote revocation failed (device offline or
	// server rejected it) but the local sign-out succeeded, so the app can
	// return to the terminal. The device stays linked on the phone until the
	// user removes it manually from WhatsApp → Linked devices.
	logoutStatusLocalOnly uint8 = 3
)

//export C_Logout
func C_Logout() C.uint8_t {
	status := logoutStatusLoggedOut
	if client == nil {
		// No bridge client exists, so there is no session to remove.
		status = logoutStatusNotLoggedIn
	} else {
		// True sign-out: whatsmeow sends the remove-companion-device IQ to the
		// server (which unlinks the device in WhatsApp on the phone), then
		// disconnects and clears the persisted store. Bounded so it can never
		// hang the UI; the Rust side keeps driving the exit.
		ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
		err := client.Logout(ctx)
		cancel()
		switch {
		case err == nil:
			status = logoutStatusLoggedOut
		case errors.Is(err, whatsmeow.ErrNotLoggedIn):
			status = logoutStatusNotLoggedIn
		default:
			// Remote revocation failed. Still clear the local session so the
			// TUI deterministically returns to the terminal; report the partial
			// outcome so the caller can warn that the device remains linked.
			LOG_ERROR("Logout: remote revocation failed, clearing locally only: %v", err)
			client.Disconnect()
			if client.Store == nil {
				status = logoutStatusLocalOnly
			} else if err := client.Store.Delete(context.Background()); err != nil {
				LOG_ERROR("Logout: failed to clear local store: %v", err)
				status = logoutStatusFailed
			} else {
				status = logoutStatusLocalOnly
			}
		}
	}
	emitLogoutResult(status)
	return C.uint8_t(status)
}

func emitLogoutResult(status uint8) {
	if eventHandler.callback == nil {
		return
	}
	payload := (*C.LogoutResultEvent)(C.malloc(C.sizeof_LogoutResultEvent))
	if payload == nil {
		return
	}
	payload.status = C.uint8_t(status)
	C.callEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeLogoutResult),
		data: unsafe.Pointer(payload),
	})
	C.free(unsafe.Pointer(payload))
}

//export C_DrainRawPresenceDiagnostics
func C_DrainRawPresenceDiagnostics() *C.char {
	report := rawPresenceProbe.drain()
	if report == "" {
		return nil
	}
	return C.CString(report)
}

//export C_FreeRawPresenceDiagnostics
func C_FreeRawPresenceDiagnostics(report *C.char) {
	C.free(unsafe.Pointer(report))
}

func main() {} // Required for CGO
