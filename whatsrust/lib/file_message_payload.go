package main

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include "callback_log_registration.h"

typedef const char* JID;
typedef struct {
	uintptr_t start;
	uintptr_t end;
} MentionRange;

	typedef struct {
		uint8_t kind;
		char* path;
		char* fileID;
		char* caption;
		JID* mentionedJIDs;
		uintptr_t mentionedCount;
		MentionRange* mentionRanges;
		uintptr_t mentionRangeCount;
	} FileMessage;

extern void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data);
*/
import "C"

import (
	"context"
	"fmt"
	"strings"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func emitFileMessage(cinfo C.MessageInfo, kind uint8, filePath, fileID, caption string, isSync bool) {
	cfileID := C.CString(fileID)
	defer C.free(unsafe.Pointer(cfileID))

	cpath := C.CString(filePath)
	defer C.free(unsafe.Pointer(cpath))

	ccaption := C.CString(caption)
	if caption == "" {
		ccaption = nil
	}
	defer C.free(unsafe.Pointer(ccaption))

	content := (*C.FileMessage)(C.malloc(C.sizeof_FileMessage))
	content.mentionRanges = nil
	content.mentionRangeCount = 0
	content.kind = C.uint8_t(kind)
	content.path = cpath
	content.fileID = cfileID
	content.caption = ccaption
	content.mentionedJIDs = nil
	content.mentionedCount = 0
	ranges := takePendingMentionRanges(caption)
	memory, mentionRanges, mentionRangeCount := buildFileMessageMentionRanges(ranges)
	if memory != nil {
		content.mentionRanges = mentionRanges
		content.mentionRangeCount = mentionRangeCount
		defer C.free(memory)
	}
	defer C.free(unsafe.Pointer(content))

	message := C.Message{
		info:        cinfo,
		messageType: C.uint8_t(MessageTypeFile),
		message:     unsafe.Pointer(content),
	}
	C.callMessageHandler(messageHandler, C.bool(isSync), &message)
}

func buildFileMessageMentionRanges(ranges []mentionRange) (unsafe.Pointer, *C.MentionRange, C.uintptr_t) {
	var mentionRanges *C.MentionRange
	var mentionRangeCount C.uintptr_t
	if len(ranges) == 0 {
		return nil, mentionRanges, mentionRangeCount
	}

	memory := C.malloc(C.size_t(len(ranges)) * C.sizeof_MentionRange)
	mentionRanges = (*C.MentionRange)(memory)
	mentionRangeCount = C.uintptr_t(len(ranges))
	entries := unsafe.Slice(mentionRanges, len(ranges))
	for index, mention := range ranges {
		entries[index].start = C.uintptr_t(mention.Start)
		entries[index].end = C.uintptr_t(mention.End)
	}
	return memory, mentionRanges, mentionRangeCount
}

func emitImageMessage(cinfo C.MessageInfo, messageID string, image *waE2E.ImageMessage, isSync bool) bool {
	if image == nil {
		LOG_ERROR("ImageMessage is nil")
		return false
	}
	setMessageQuoteID(&cinfo, image.GetContextInfo())
	ext := ExtensionByType(image.GetMimetype(), ".jpg")
	filePath := fmt.Sprintf("imgs/%s%s", messageID, ext)
	fileID := DownloadableMessageToFileId(client, image, filePath)
	emitFileMessage(cinfo, FileTypeImage, filePath, fileID, captionWithMentionNames(image.GetCaption(), image.GetContextInfo(), cinfo), isSync)
	return true
}

func emitVideoMessage(cinfo C.MessageInfo, messageID string, video *waE2E.VideoMessage, isSync bool) bool {
	if video == nil {
		LOG_ERROR("VideoMessage is nil")
		return false
	}
	setMessageQuoteID(&cinfo, video.GetContextInfo())
	ext := ExtensionByType(video.GetMimetype(), ".mp4")
	filePath := fmt.Sprintf("videos/%s%s", messageID, ext)
	fileID := DownloadableMessageToFileId(client, video, filePath)
	if thumbnail := video.GetJPEGThumbnail(); len(thumbnail) > 0 {
		thumbPath := strings.TrimSuffix(filePath, ext) + ".jpg"
		fileID = AddThumbnailToFileId(fileID, thumbnail, thumbPath)
	}
	emitFileMessage(cinfo, FileTypeVideo, filePath, fileID, captionWithMentionNames(video.GetCaption(), video.GetContextInfo(), cinfo), isSync)
	return true
}

func emitAudioMessage(cinfo C.MessageInfo, messageID string, audio *waE2E.AudioMessage, isSync bool) bool {
	if audio == nil {
		LOG_ERROR("AudioMessage is nil")
		return false
	}
	setMessageQuoteID(&cinfo, audio.GetContextInfo())
	ext := ExtensionByType(audio.GetMimetype(), ".ogg")
	filePath := fmt.Sprintf("audios/%s%s", messageID, ext)
	fileID := DownloadableMessageToFileId(client, audio, filePath)
	emitFileMessage(cinfo, FileTypeAudio, filePath, fileID, "", isSync)
	return true
}

func emitDocumentMessage(cinfo C.MessageInfo, messageID string, document *waE2E.DocumentMessage, isSync bool) bool {
	if document == nil {
		LOG_ERROR("DocumentMessage is nil")
		return false
	}
	setMessageQuoteID(&cinfo, document.GetContextInfo())
	filePath := fmt.Sprintf("docs/%s-%s", messageID, *document.FileName)
	fileID := DownloadableMessageToFileId(client, document, filePath)
	emitFileMessage(cinfo, FileTypeDocument, filePath, fileID, captionWithMentionNames(document.GetCaption(), document.GetContextInfo(), cinfo), isSync)
	return true
}

func captionWithMentionNames(caption string, contextInfo *waE2E.ContextInfo, cinfo C.MessageInfo) string {
	if contextInfo == nil || caption == "" {
		return caption
	}
	chat, err := types.ParseJID(C.GoString(cinfo.chat))
	if err != nil {
		return caption
	}
	return replaceMentionedNames(
		caption,
		contextInfo.GetMentionedJID(),
		mentionEntriesForGroup(context.Background(), chat, contextInfo.GetMentionedJID()...),
	)
}

func emitStickerMessage(cinfo C.MessageInfo, messageID string, sticker *waE2E.StickerMessage, isSync bool) bool {
	if sticker == nil {
		LOG_ERROR("StickerMessage is nil")
		return false
	}
	setMessageQuoteID(&cinfo, sticker.GetContextInfo())
	ext := ExtensionByType(sticker.GetMimetype(), ".webp")
	filePath := fmt.Sprintf("stickers/%s%s", messageID, ext)
	fileID := DownloadableMessageToFileId(client, sticker, filePath)
	emitFileMessage(cinfo, FileTypeSticker, filePath, fileID, "", isSync)
	return true
}

func setMessageQuoteID(cinfo *C.MessageInfo, contextInfo *waE2E.ContextInfo) {
	if contextInfo != nil && contextInfo.GetStanzaID() != "" {
		cinfo.quoteID = C.CString(contextInfo.GetStanzaID())
	}
}
