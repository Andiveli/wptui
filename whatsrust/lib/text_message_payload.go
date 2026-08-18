package main

/*
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>
#include "callback_log_registration.h"

typedef struct {
	char* text;
} TextMessage;

extern void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data);
*/
import "C"

import "unsafe"

func emitTextMessage(cinfo C.MessageInfo, text string, isSync bool) {
	ctext := C.CString(text)
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
