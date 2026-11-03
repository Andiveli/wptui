package main

/*
#include "callback_log_registration.h"
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct {
	JID chat;
	char* messageID;
	bool read;
	int64_t timestamp;
	bool fromMe;
	JID participant;
} MarkChatAsReadEvent;

static void callMarkChatAsReadEventCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
*/
import "C"

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"time"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

type markChatAsReadEvent struct {
	chat        string
	messageID   string
	read        bool
	timestamp   int64
	fromMe      bool
	participant string
}

func markChatAsReadEventFromEvent(event *events.MarkChatAsRead) (markChatAsReadEvent, bool) {
	if event == nil || event.Action == nil || event.JID.IsEmpty() {
		return markChatAsReadEvent{}, false
	}
	rangeInfo := event.Action.MessageRange
	if event.Action.GetRead() {
		if rangeInfo == nil || rangeInfo.LastMessageTimestamp == nil {
			return markChatAsReadEvent{}, false
		}
		if rangeInfo.GetLastMessageTimestamp() == 0 && len(rangeInfo.GetMessages()) > 0 {
			return markChatAsReadEvent{}, false
		}
	}
	result := markChatAsReadEvent{
		chat:      event.JID.String(),
		read:      event.Action.GetRead(),
		timestamp: rangeInfo.GetLastMessageTimestamp(),
	}
	if messages := rangeInfo.GetMessages(); len(messages) > 0 && messages[len(messages)-1] != nil {
		key := messages[len(messages)-1].GetKey()
		if key != nil {
			result.messageID = key.GetID()
			result.fromMe = key.GetFromMe()
			result.participant = key.GetParticipant()
		}
	}
	return result, true
}

func dispatchMarkChatAsReadEvent(event *events.MarkChatAsRead) {
	parsed, ok := markChatAsReadEventFromEvent(event)
	if !ok || eventHandler.callback == nil {
		return
	}
	chat := C.CString(parsed.chat)
	messageID := C.CString(parsed.messageID)
	defer C.free(unsafe.Pointer(chat))
	defer C.free(unsafe.Pointer(messageID))
	var participant *C.char
	if parsed.participant != "" {
		participant = C.CString(parsed.participant)
		defer C.free(unsafe.Pointer(participant))
	}
	payload := (*C.MarkChatAsReadEvent)(C.malloc(C.sizeof_MarkChatAsReadEvent))
	if payload == nil {
		return
	}
	payload.chat = chat
	payload.messageID = messageID
	payload.read = C.bool(parsed.read)
	payload.timestamp = C.int64_t(parsed.timestamp)
	payload.fromMe = C.bool(parsed.fromMe)
	payload.participant = participant
	C.callMarkChatAsReadEventCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeMarkChatAsRead), data: unsafe.Pointer(payload)})
	C.free(unsafe.Pointer(payload))
}

// Only this formatter can produce read-sync bridge records. Never forward err text.
// The optional stage is inert until the stage grammar has behavioral RED evidence.
func markReadResultWithTrace(send func() error, output io.Writer, stages ...string) C.int {
	if output == nil {
		return markReadResult(send)
	}
	var err error
	wrapped := send
	if send != nil {
		wrapped = func() error { err = send(); return err }
	}
	status := markReadResult(wrapped)
	category := "none"
	if send == nil {
		category = "unavailable"
	} else if err != nil {
		var iq *whatsmeow.IQError
		var missing *whatsmeow.ElementMissingError
		var disconnected *whatsmeow.DisconnectedError
		isIQ := errors.As(err, &iq)
		isMissing := errors.As(err, &missing)
		isDisconnected := errors.As(err, &disconnected)
		switch {
		// A typed nil must not reach Error() or a field dereference.
		case (isIQ && iq == nil) || (isMissing && missing == nil) || (isDisconnected && disconnected == nil):
			category = "other"
		case strings.HasPrefix(err.Error(), "failed to fetch app state after sending update:"):
			category = "resync_after_send"
		case err.Error() == "no app state keys found, creating app state keys is not yet supported":
			category = "missing_key"
		case strings.HasPrefix(err.Error(), "failed to get latest app state key ID:"):
			category = "key_lookup"
		case errors.Is(err, context.Canceled):
			category = "cancelled"
		case errors.Is(err, context.DeadlineExceeded):
			category = "timeout"
		// Keep follow-up resync classification above all typed errors: the
		// mutation may already have applied. Only closed labels leave Go.
		case errors.Is(err, whatsmeow.ErrAppStateUpdate):
			category = "appstate_rejected"
			var multiple interface{ Unwrap() []error }
			if errors.As(err, &multiple) {
				causes := multiple.Unwrap()
				// Inspect only the public secondary branch, never its text.
				// A joined error alone does not prove a server code or conflict.
				if len(causes) == 2 && errors.Is(causes[0], whatsmeow.ErrAppStateUpdate) && causes[1] != nil {
					category = "appstate_with_cause"
					switch {
					case errors.Is(causes[1], appstate.ErrKeyNotFound):
						category = "appstate_cause_key_missing"
					case errors.Is(causes[1], appstate.ErrMismatchingContentMAC):
						category = "appstate_cause_content_mac"
					case errors.Is(causes[1], appstate.ErrMismatchingIndexMAC):
						category = "appstate_cause_index_mac"
					case errors.Is(causes[1], appstate.ErrMismatchingLTHash):
						category = "appstate_cause_lt_hash"
					case errors.Is(causes[1], appstate.ErrMismatchingPatchMAC):
						category = "appstate_cause_patch_mac"
					case errors.Is(causes[1], appstate.ErrMissingPreviousSetValueOperation):
						category = "appstate_cause_previous_set_missing"
					}
				}
			}
		case isIQ:
			switch iq.Code {
			case 400:
				category = "iq_bad_request"
			case 401:
				category = "iq_not_authorized"
			case 403:
				category = "iq_forbidden"
			case 404:
				category = "iq_not_found"
			case 405:
				category = "iq_not_allowed"
			case 406:
				category = "iq_not_acceptable"
			case 410:
				category = "iq_gone"
			case 419:
				category = "iq_resource_limit"
			case 423:
				category = "iq_locked"
			case 429:
				category = "iq_rate_over_limit"
			case 500:
				category = "iq_internal_server_error"
			case 503:
				category = "iq_service_unavailable"
			case 530:
				category = "iq_partial_server_error"
			default:
				category = "iq_other"
			}
		case isMissing:
			category = "missing_element"
		case isDisconnected || errors.Is(err, whatsmeow.ErrNotConnected):
			category = "disconnected"
		case errors.Is(err, whatsmeow.ErrNotLoggedIn):
			category = "not_logged_in"
		case errors.Is(err, whatsmeow.ErrIQTimedOut):
			category = "iq_timeout"
		case errors.Is(err, whatsmeow.ErrClientIsNil):
			category = "client_nil"
		default:
			category = "other"
		}
	}
	// Diagnostic failures must never replace the original bridge result.
	_, _ = fmt.Fprintf(output, "stage=bridge status=%d category=%s\n", status, category)
	return status
}

type readSyncTraceWriter func(string)

func (emit readSyncTraceWriter) Write(record []byte) (int, error) {
	emit(strings.TrimSuffix(string(record), "\n"))
	return len(record), nil
}

func readSyncTraceOutput(path string, emit func(string)) io.Writer {
	if path == "" {
		return nil
	}
	return readSyncTraceWriter(emit)
}

type chatReadSyncFunc func(context.Context, appstate.PatchInfo) error

func buildChatReadSyncPatch(chat, messageID string, timestamp int64, fromMe bool, participant string) (appstate.PatchInfo, error) {
	target, err := types.ParseJID(chat)
	if err != nil || target.IsEmpty() {
		return appstate.PatchInfo{}, fmt.Errorf("invalid chat JID")
	}
	if messageID == "" {
		return appstate.PatchInfo{}, fmt.Errorf("empty message ID")
	}
	return buildChatReadStateSyncPatch(chat, messageID, true, timestamp, fromMe, participant)
}

func buildChatReadStateSyncPatch(chat, messageID string, read bool, timestamp int64, fromMe bool, participant string) (appstate.PatchInfo, error) {
	target, err := types.ParseJID(chat)
	if err != nil || target.IsEmpty() {
		return appstate.PatchInfo{}, fmt.Errorf("invalid chat JID")
	}
	var key *waCommon.MessageKey
	if messageID != "" {
		key = &waCommon.MessageKey{RemoteJID: &chat, FromMe: &fromMe, ID: &messageID}
		if participant != "" {
			key.Participant = &participant
		}
	}
	return appstate.BuildMarkChatAsRead(target, read, time.Unix(timestamp, 0), key), nil
}

func sendChatReadSync(ctx context.Context, chat, messageID string, timestamp int64, fromMe bool, participant string, send chatReadSyncFunc) error {
	if send == nil {
		return fmt.Errorf("nil chat read sync sender")
	}
	patch, err := buildChatReadSyncPatch(chat, messageID, timestamp, fromMe, participant)
	if err != nil {
		return err
	}
	return send(ctx, patch)
}

func sendChatReadStateSync(ctx context.Context, chat, messageID string, read bool, timestamp int64, fromMe bool, participant string, send chatReadSyncFunc) error {
	if send == nil {
		return fmt.Errorf("nil chat read sync sender")
	}
	patch, err := buildChatReadStateSyncPatch(chat, messageID, read, timestamp, fromMe, participant)
	if err != nil {
		return err
	}
	return send(ctx, patch)
}

// C_MarkChatReadSync queues a chat-level app-state mutation. Status chats must
// continue using C_MarkAsRead and never call this bridge.
//
//export C_MarkChatReadSync
func C_MarkChatReadSync(chatJID *C.char, messageID *C.char, timestamp C.longlong, fromMe C.bool, participantJID C.JID) C.int {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || chatJID == nil || messageID == nil {
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	participant := ""
	if participantJID != nil {
		participant = C.GoString(participantJID)
	}
	return markReadResult(func() error {
		return sendChatReadSync(ctx, C.GoString(chatJID), C.GoString(messageID), int64(timestamp), bool(fromMe), participant,
			func(ctx context.Context, patch appstate.PatchInfo) error {
				return clientSnapshot.SendAppState(ctx, patch)
			})
	})
}

// C_MarkChatReadStateSync sends chat-level app-state, never message receipts.
// A nil messageID represents an empty chat; a non-nil ID must not be empty.
// Status chats must continue using C_MarkAsRead instead.
//
//export C_MarkChatReadStateSync
func C_MarkChatReadStateSync(chatJID *C.char, messageID *C.char, read C.bool, timestamp C.longlong, fromMe C.bool, participantJID C.JID) C.int {
	// The existing callback routes this safe record into the app's single capped
	// report. Go never opens the path or writes a second, independently capped file.
	output := readSyncTraceOutput(os.Getenv("WPTUI_READ_SYNC_TRACE"), func(record string) {
		LOG_DEBUG("READ_SYNC_TRACE %s", record)
	})
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || chatJID == nil {
		return markReadResultWithTrace(nil, output)
	}
	id := ""
	if messageID != nil {
		id = C.GoString(messageID)
		if id == "" {
			return markReadResultWithTrace(func() error { return fmt.Errorf("empty message ID") }, output)
		}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	participant := ""
	if participantJID != nil {
		participant = C.GoString(participantJID)
	}
	return markReadResultWithTrace(func() error {
		return sendChatReadStateSync(ctx, C.GoString(chatJID), id, bool(read), int64(timestamp), bool(fromMe), participant,
			func(ctx context.Context, patch appstate.PatchInfo) error {
				return sendChatReadStatePatch(ctx, patch, clientSnapshot,
					func(ctx context.Context, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
						return lifecycleState.withChatReadRecovery(ctx, clientSnapshot, name, operation)
					})
			})
	}, output)
}
