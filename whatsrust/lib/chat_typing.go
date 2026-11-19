package main

/*
#include <stdlib.h>
#include "callback_log_registration.h"

typedef struct {
	JID chat;
	JID sender;
	bool composing;
} ChatTypingEvent;

static void callChatTypingCallback(EventHandler handler, const Event* event) {
	handler.callback(event, handler.user_data);
}
*/
import "C"

import (
	"unsafe"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

type chatTypingEvent struct {
	chat, sender string
	composing    bool
}

func chatTypingFromPresence(event *events.ChatPresence) (chatTypingEvent, bool) {
	if event.IsFromMe || event.Chat.IsEmpty() || event.Sender.IsEmpty() {
		return chatTypingEvent{}, false
	}
	switch event.Chat.Server {
	case types.DefaultUserServer, types.HiddenUserServer, types.GroupServer:
	default:
		return chatTypingEvent{}, false
	}
	switch event.State {
	case types.ChatPresenceComposing, types.ChatPresencePaused:
	default:
		return chatTypingEvent{}, false
	}
	return chatTypingEvent{
		chat:      event.Chat.ToNonAD().String(),
		sender:    event.Sender.ToNonAD().String(),
		composing: event.State == types.ChatPresenceComposing && event.Media == types.ChatPresenceMediaText,
	}, true
}

func dispatchChatTyping(event *events.ChatPresence) {
	typing, ok := chatTypingFromPresence(event)
	if !ok || eventHandler.callback == nil {
		return
	}
	chat := C.CString(typing.chat)
	defer C.free(unsafe.Pointer(chat))
	sender := C.CString(typing.sender)
	defer C.free(unsafe.Pointer(sender))
	payload := (*C.ChatTypingEvent)(C.malloc(C.sizeof_ChatTypingEvent))
	if payload == nil {
		return
	}
	defer C.free(unsafe.Pointer(payload))
	payload.chat = chat
	payload.sender = sender
	payload.composing = C.bool(typing.composing)
	C.callChatTypingCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeChatTyping),
		data: unsafe.Pointer(payload),
	})
}
