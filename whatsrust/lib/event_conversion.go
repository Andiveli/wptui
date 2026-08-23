package main

/*
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;

typedef struct {
	JID chat;
	char* targetMessageID;
	JID participant;
	char* text;
	bool isFromMe;
} ReactionEvent;

typedef struct {
	uint8_t kind;
	void* data;
} Event;

typedef void (*EventCallback)(const Event*, void*);
typedef struct {
	EventCallback callback;
	void* user_data;
} EventHandler;

static void callEventConversionCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
*/
import "C"

import (
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

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

	C.callEventConversionCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeReaction), data: unsafe.Pointer(payload)})
	C.free(unsafe.Pointer(payload))
}

func reactionEventFromMessage(info types.MessageInfo, msg *waE2E.Message) (reactionEvent, bool) {
	reaction := msg.GetReactionMessage()
	if reaction == nil || reaction.GetKey() == nil || reaction.GetKey().GetID() == "" {
		return reactionEvent{}, false
	}
	return reactionEvent{chat: info.Chat.String(), targetMessageID: reaction.GetKey().GetID(), participant: info.Sender.String(), text: reaction.GetText(), isFromMe: info.IsFromMe}, true
}
