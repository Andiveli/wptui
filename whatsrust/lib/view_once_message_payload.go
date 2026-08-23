package main

/*
#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>
#include "callback_log_registration.h"

typedef struct {
	uint8_t reserved;
} ViewOnceUnavailableMessage;

extern void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data);
*/
import "C"

import "unsafe"

func emitViewOnceUnavailableMessage(cinfo C.MessageInfo, isSync bool) {
	content := (*C.ViewOnceUnavailableMessage)(C.malloc(C.sizeof_ViewOnceUnavailableMessage))
	if content == nil {
		return
	}
	content.reserved = 0
	defer C.free(unsafe.Pointer(content))

	message := C.Message{
		info:        cinfo,
		messageType: C.uint8_t(MessageTypeViewOnceUnavailable),
		message:     unsafe.Pointer(content),
	}
	C.callMessageHandler(messageHandler, C.bool(isSync), &message)
}
