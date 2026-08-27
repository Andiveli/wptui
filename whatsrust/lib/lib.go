package main

import (
	"context"
	"os"

	"go.mau.fi/whatsmeow"
)

const (
	MediaImage    whatsmeow.MediaType = "WhatsApp Image Keys"
	MediaVideo    whatsmeow.MediaType = "WhatsApp Video Keys"
	MediaAudio    whatsmeow.MediaType = "WhatsApp Audio Keys"
	MediaDocument whatsmeow.MediaType = "WhatsApp Document Keys"
	MediaHistory  whatsmeow.MediaType = "WhatsApp History Keys"
	MediaAppState whatsmeow.MediaType = "WhatsApp App State Keys"

	MediaLinkThumbnail whatsmeow.MediaType = "WhatsApp Link Thumbnail Keys"
)

var mediaTypeToMMSType = map[whatsmeow.MediaType]string{
	MediaImage:    "image",
	MediaAudio:    "audio",
	MediaVideo:    "video",
	MediaDocument: "document",
	MediaHistory:  "md-msg-hist",
	MediaAppState: "md-app-state",

	MediaLinkThumbnail: "thumbnail-link",
}

const (
	FileStatusNone = iota - 1
	FileStatusDownloaded
	FileStatusDownloadFailed
)

// TODO: Implement URL download
func DownloadFromFileInfo(client *whatsmeow.Client, info DownloadInfo) ([]byte, error) {
	return client.DownloadMediaWithPath(
		context.Background(),
		info.DirectPath,
		info.FileEncSha256,
		info.FileSha256,
		info.MediaKey,
		info.MediaType,
		mediaTypeToMMSType[info.MediaType],
		false,
	)
}
func DownloadFromFileId(client *whatsmeow.Client, fileId string, basePath string) int {
	if client == nil {
		return FileStatusDownloadFailed
	}
	info, err := FileIdToDownloadInfo(fileId)
	if err != nil {
		return FileStatusDownloadFailed
	}

	target, err := safeDownloadTarget(basePath, info.TargetPath)
	if err != nil {
		return FileStatusDownloadFailed
	}

	alreadyDownloaded := false
	if _, err := os.Stat(target); err == nil {
		alreadyDownloaded = true
	}

	if !alreadyDownloaded {
		data, err := DownloadFromFileInfo(client, info)
		if err != nil {
			return FileStatusDownloadFailed
		}
		if err := writeDownload(basePath, info.TargetPath, data); err != nil {
			return FileStatusDownloadFailed
		}
	}

	// Write thumbnail sidecar if available (skips if already exists).
	if len(info.ThumbnailData) > 0 && info.ThumbnailTargetPath != "" {
		thumbTarget, err := safeDownloadTarget(basePath, info.ThumbnailTargetPath)
		if err == nil {
			if _, err := os.Stat(thumbTarget); err != nil {
				_ = writeDownload(basePath, info.ThumbnailTargetPath, info.ThumbnailData)
			}
		}
	}

	return FileStatusDownloaded
}
