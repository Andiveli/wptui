package main

/*
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>
#include "callback_log_registration.h"

typedef struct {
	uint8_t kind;
	char* path;
	char* fileID;
	char* caption;
} FileMessage;

extern void callMessageHandler(MessageHandler hdl, bool isSync, const Message* data);
*/
import "C"

import (
	"fmt"
	"strings"
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
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
	content.kind = C.uint8_t(kind)
	content.path = cpath
	content.fileID = cfileID
	content.caption = ccaption
	defer C.free(unsafe.Pointer(content))

	message := C.Message{
		info:        cinfo,
		messageType: C.uint8_t(MessageTypeFile),
		message:     unsafe.Pointer(content),
	}
	C.callMessageHandler(messageHandler, C.bool(isSync), &message)
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
	emitFileMessage(cinfo, FileTypeImage, filePath, fileID, image.GetCaption(), isSync)
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
	emitFileMessage(cinfo, FileTypeVideo, filePath, fileID, video.GetCaption(), isSync)
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
	emitFileMessage(cinfo, FileTypeDocument, filePath, fileID, document.GetCaption(), isSync)
	return true
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
