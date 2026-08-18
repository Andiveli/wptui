package main

/*
#include <stdint.h>
#include <stdlib.h>

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

static void callConnectionEventCallback(EventHandler hdl, const Event* event) {
    hdl.callback(event, hdl.user_data);
}
*/
import "C"

import "unsafe"

func dispatchConnectedEvent() {
	if eventHandler.callback == nil {
		return
	}
	C.callConnectionEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeConnected),
		data: nil,
	})
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
	C.callConnectionEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeLogoutResult),
		data: unsafe.Pointer(payload),
	})
	C.free(unsafe.Pointer(payload))
}
