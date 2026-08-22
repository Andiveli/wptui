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
	"fmt"
	"time"
	"unsafe"

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
	if event == nil || event.Action == nil || event.Action.MessageRange == nil {
		return markChatAsReadEvent{}, false
	}
	rangeInfo := event.Action.MessageRange
	if event.JID.IsEmpty() || rangeInfo.GetLastMessageTimestamp() == 0 {
		return markChatAsReadEvent{}, false
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

type chatReadSyncFunc func(context.Context, appstate.PatchInfo) error

func buildChatReadSyncPatch(chat, messageID string, timestamp int64, fromMe bool, participant string) (appstate.PatchInfo, error) {
	target, err := types.ParseJID(chat)
	if err != nil || target.IsEmpty() {
		return appstate.PatchInfo{}, fmt.Errorf("invalid chat JID")
	}
	if messageID == "" {
		return appstate.PatchInfo{}, fmt.Errorf("empty message ID")
	}
	remoteJID := chat
	id := messageID
	key := &waCommon.MessageKey{RemoteJID: &remoteJID, FromMe: &fromMe, ID: &id}
	if participant != "" {
		key.Participant = &participant
	}
	return appstate.BuildMarkChatAsRead(target, true, time.Unix(timestamp, 0), key), nil
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

// C_MarkChatReadSync queues a chat-level app-state mutation. Status chats must
// continue using C_MarkAsRead and never call this bridge.
//
//export C_MarkChatReadSync
func C_MarkChatReadSync(chatJID *C.char, messageID *C.char, timestamp C.longlong, fromMe C.bool, participantJID C.JID) C.int {
	if client == nil || chatJID == nil || messageID == nil {
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
				return client.SendAppState(ctx, patch)
			})
	})
}
