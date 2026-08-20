package main

/*
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>
#include "callback_log_registration.h"

typedef struct {
	uintptr_t start;
	uintptr_t end;
} MentionRange;

typedef struct {
	char* text;
} TextMessage;

typedef struct {
	char* text;
	MentionRange* mentionRanges;
	uintptr_t mentionRangeCount;
} MentionedTextMessage;

extern void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data);
*/
import "C"

import "unsafe"

func emitTextMessage(cinfo C.MessageInfo, text string, isSync bool) {
	ctext := C.CString(text)
	defer C.free(unsafe.Pointer(ctext))

	ranges := takePendingMentionRanges(text)
	var cranges *C.MentionRange
	if len(ranges) > 0 {
		memory := C.malloc(C.size_t(len(ranges)) * C.sizeof_MentionRange)
		cranges = (*C.MentionRange)(memory)
		entries := unsafe.Slice(cranges, len(ranges))
		for index, mention := range ranges {
			entries[index].start = C.uintptr_t(mention.Start)
			entries[index].end = C.uintptr_t(mention.End)
		}
		defer C.free(memory)
	}

	content := (*C.MentionedTextMessage)(C.malloc(
		C.sizeof_MentionedTextMessage + 0*C.sizeof_TextMessage,
	))
	content.text = ctext
	content.mentionRanges = cranges
	content.mentionRangeCount = C.uintptr_t(len(ranges))
	defer C.free(unsafe.Pointer(content))

	message := C.Message{
		info:        cinfo,
		messageType: C.uint8_t(MessageTypeText),
		message:     unsafe.Pointer(content),
	}
	C.callMessageHandler(messageHandler, C.bool(isSync), &message)
}
