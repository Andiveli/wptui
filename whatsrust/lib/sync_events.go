package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;

typedef struct {
	JID chat;
	int64_t lastMessageTime;
} ChatEvent;

typedef struct {
	uint8_t kind;
	void* data;
} Event;

typedef void (*EventCallback)(const Event*, void*);
typedef struct {
	EventCallback callback;
	void* user_data;
} EventHandler;

static void callSyncEventCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
*/
import "C"

import (
	"context"
	"unsafe"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waHistorySync"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

type syncMessageDispatcher func(*events.Message)

type chatSyncEvent struct {
	chat            types.JID
	lastMessageTime int64
}

func appStateSyncComplete(evt *events.AppStateSyncComplete) bool {
	return evt != nil && evt.Name == appstate.WAPatchRegular
}

func chatSyncEventFromConversation(conversation *waHistorySync.Conversation) (chatSyncEvent, bool) {
	if conversation == nil {
		return chatSyncEvent{}, false
	}
	chatJID, err := types.ParseJID(conversation.GetID())
	if err != nil {
		return chatSyncEvent{}, false
	}
	var lastMessageTime int64
	if timestamp := conversation.GetLastMsgTimestamp(); timestamp != 0 {
		lastMessageTime = int64(timestamp)
	}
	return chatSyncEvent{chat: chatJID, lastMessageTime: lastMessageTime}, true
}

func countSyncMessages(conversations []*waHistorySync.Conversation) int {
	total := 0
	for _, conversation := range conversations {
		total += len(conversation.GetMessages())
	}
	return total
}

func dispatchAppStateSyncComplete(evt *events.AppStateSyncComplete) {
	if evt == nil {
		return
	}
	LOG_INFO("AppStateSyncComplete %v", evt)
	if !appStateSyncComplete(evt) {
		return
	}
	LOG_INFO("AppStateSyncComplete (WAPatchRegular) %v", evt)
	C.callSyncEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeAppStateSyncComplete),
		data: nil,
	})
}

func dispatchHistorySync(
	evt *events.HistorySync,
	storeHistoricalSecrets func(context.Context, []*waHistorySync.Conversation),
	parseWebMessage func(types.JID, *waWeb.WebMessageInfo) (*events.Message, error),
	dispatchMessage syncMessageDispatcher,
) {
	percent := evt.Data.GetProgress()
	conversations := evt.Data.GetConversations()
	LOG_INFO(
		"History sync: type=%s chunk=%d progress=%d conversations=%d messages=%d",
		evt.Data.GetSyncType().String(),
		evt.Data.GetChunkOrder(),
		percent,
		len(conversations),
		countSyncMessages(conversations),
	)
	cpercent := (*C.uint8_t)(C.malloc(C.size_t(unsafe.Sizeof(uint8(0)))))
	*cpercent = C.uint8_t(percent)
	C.callSyncEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeSyncProgress),
		data: unsafe.Pointer(cpercent),
	})
	C.free(unsafe.Pointer(cpercent))

	// Store secrets before parsing encrypted history edits.
	storeHistoricalSecrets(context.Background(), conversations)
	for _, conversation := range conversations {
		chatEvent, ok := chatSyncEventFromConversation(conversation)
		if !ok {
			LOG_WARN("history message ignored source=history_sync reason=invalid_chat")
			continue
		}
		// Register chats even when a sync batch carries no messages.
		payload := (*C.ChatEvent)(C.malloc(C.sizeof_ChatEvent))
		payload.chat = jidToC(chatEvent.chat)
		payload.lastMessageTime = C.int64_t(chatEvent.lastMessageTime)
		C.callSyncEventCallback(eventHandler, &C.Event{
			kind: C.uint8_t(EventTypeChat),
			data: unsafe.Pointer(payload),
		})
		C.free(unsafe.Pointer(payload.chat))
		C.free(unsafe.Pointer(payload))

		for _, syncMessage := range conversation.GetMessages() {
			webMessageInfo := syncMessage.Message
			if webMessageInfo == nil {
				continue
			}
			parsed, err := parseWebMessage(chatEvent.chat, webMessageInfo)
			if err != nil {
				LOG_WARN("history message ignored source=history_sync reason=parse_failed")
				continue
			}
			dispatchMessage(parsed)
		}
	}
}
