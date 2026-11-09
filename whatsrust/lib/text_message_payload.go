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
	NativeLinkPreviewPayload* nativePreview;
} MentionedTextMessage;

extern void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data);
extern void callOptimisticTextSentHandler(OptimisticTextSentHandler hdl, uint64_t localSendID, const Message* data);
*/
import "C"

import "unsafe"

type textCallbackOutput struct {
	localSendID   uint64
	text          string
	ranges        []mentionRange
	mentionsSelf  bool
	quoteID       string
	nativePreview *nativeLinkPreview
}

var observeTextCallback = func(textCallbackOutput) {}

func notifyTextCallback(cinfo C.MessageInfo, text string, ranges []mentionRange, localSendID uint64, preview *nativeLinkPreview) {
	quoteID := ""
	if cinfo.quoteID != nil {
		quoteID = C.GoString(cinfo.quoteID)
	}
	observeTextCallback(textCallbackOutput{
		localSendID:   localSendID,
		text:          text,
		ranges:        append([]mentionRange(nil), ranges...),
		mentionsSelf:  bool(cinfo.mentionsSelf),
		quoteID:       quoteID,
		nativePreview: preview,
	})
}

// Bind optional metadata to this emission only, without a pending global cache.
func nativeTextEmitter(preview *nativeLinkPreview) func(C.MessageInfo, string, bool) {
	return func(info C.MessageInfo, text string, isSync bool) {
		emitTextMessage(info, text, isSync, preview)
	}
}

func emitTextMessage(cinfo C.MessageInfo, text string, isSync bool, previews ...*nativeLinkPreview) {
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
	content.nativePreview = nil
	var preview *nativeLinkPreview
	if len(previews) != 0 {
		preview = previews[0]
	}
	if preview != nil {
		// All sizes were checked before allocating C memory. Every buffer stays
		// live through callMessageHandler and is freed only after it returns.
		matched := C.CString(preview.matchedText)
		title := C.CString(preview.title)
		description := C.CString(preview.description)
		defer C.free(unsafe.Pointer(matched))
		defer C.free(unsafe.Pointer(title))
		defer C.free(unsafe.Pointer(description))
		var thumbnail unsafe.Pointer
		if len(preview.jpegThumbnail) != 0 {
			thumbnail = C.CBytes(preview.jpegThumbnail)
			defer C.free(thumbnail)
		}
		native := (*C.NativeLinkPreviewPayload)(C.malloc(C.sizeof_NativeLinkPreviewPayload))
		*native = C.NativeLinkPreviewPayload{
			matchedText: matched, matchedTextLen: C.size_t(len(preview.matchedText)),
			title: title, titleLen: C.size_t(len(preview.title)),
			description: description, descriptionLen: C.size_t(len(preview.description)),
			jpegThumbnail: (*C.uint8_t)(thumbnail), jpegThumbnailLen: C.size_t(len(preview.jpegThumbnail)),
		}
		content.nativePreview = native
		defer C.free(unsafe.Pointer(native))
	}
	defer C.free(unsafe.Pointer(content))
	notifyTextCallback(cinfo, text, ranges, 0, preview)

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
	content.nativePreview = nil
	defer C.free(unsafe.Pointer(content))
	notifyTextCallback(cinfo, text, ranges, localSendID, nil)
	message := C.Message{info: cinfo, messageType: C.uint8_t(MessageTypeText), message: unsafe.Pointer(content)}
	if optimisticTextSentHandler.callback != nil {
		C.callOptimisticTextSentHandler(optimisticTextSentHandler, C.uint64_t(localSendID), &message)
	}
}
