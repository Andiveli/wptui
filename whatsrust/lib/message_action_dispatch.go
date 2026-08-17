package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;

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
	uint8_t kind;
	void* data;
} Event;

typedef void (*EventCallback)(const Event*, void*);
typedef struct {
	EventCallback callback;
	void* user_data;
} EventHandler;

static void callMessageActionDispatchCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
*/
import "C"

import "unsafe"

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
	C.callMessageActionDispatchCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeMessageAction), data: unsafe.Pointer(payload)})
	C.free(unsafe.Pointer(payload))
}
