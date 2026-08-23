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
extern void callOptimisticTextSentHandler(OptimisticTextSentHandler hdl, uint64_t localSendID, const Message* data);
*/
import "C"

import "unsafe"

type textCallbackOutput struct {
	localSendID uint64
	text        string
	ranges      []mentionRange
	mentionsSelf bool
	quoteID     string
}

var observeTextCallback = func(textCallbackOutput) {}

func notifyTextCallback(cinfo C.MessageInfo, text string, ranges []mentionRange, localSendID uint64) {
	quoteID := ""
	if cinfo.quoteID != nil {
		quoteID = C.GoString(cinfo.quoteID)
	}
	observeTextCallback(textCallbackOutput{
		localSendID:  localSendID,
		text:         text,
		ranges:       append([]mentionRange(nil), ranges...),
		mentionsSelf: bool(cinfo.mentionsSelf),
		quoteID:      quoteID,
	})
}

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
	notifyTextCallback(cinfo, text, ranges, 0)

	message := C.Message{
		info:        cinfo,
		messageType: C.uint8_t(MessageTypeText),
		message:     unsafe.Pointer(content),
	}
	if messageHandler.callback != nil {
		C.callMessageHandler(messageHandler, C.bool(isSync), &message)
	}
}

func emitOptimisticTextMessage(cinfo C.MessageInfo, text string, localSendID uint64) {
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
	content := (*C.MentionedTextMessage)(C.malloc(C.sizeof_MentionedTextMessage))
	content.text = ctext
	content.mentionRanges = cranges
	content.mentionRangeCount = C.uintptr_t(len(ranges))
	defer C.free(unsafe.Pointer(content))
	notifyTextCallback(cinfo, text, ranges, localSendID)
	message := C.Message{info: cinfo, messageType: C.uint8_t(MessageTypeText), message: unsafe.Pointer(content)}
	if optimisticTextSentHandler.callback != nil {
		C.callOptimisticTextSentHandler(optimisticTextSentHandler, C.uint64_t(localSendID), &message)
	}
}
