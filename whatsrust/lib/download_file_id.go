package main

import (
	"encoding/json"

	"go.mau.fi/whatsmeow"
)

type downloadableMessageWithLength interface {
	whatsmeow.DownloadableMessage
	GetFileLength() uint64
}

type downloadableMessageWithSizeBytes interface {
	whatsmeow.DownloadableMessage
	GetFileSizeBytes() uint64
}

func getSize(msg whatsmeow.DownloadableMessage) int {
	switch sized := msg.(type) {
	case downloadableMessageWithLength:
		return int(sized.GetFileLength())
	case downloadableMessageWithSizeBytes:
		return int(sized.GetFileSizeBytes())
	default:
		return -1
	}
}

var downloadInfoVersion = 1 // bump version upon any struct change

type DownloadInfo struct {
	Version int `json:"Version_int"`
	// Url        string `json:"Url_string"`
	DirectPath string `json:"DirectPath_string"`

	TargetPath string              `json:"TargetPath_string"`
	MediaKey   []byte              `json:"MediaKey_arraybyte"`
	MediaType  whatsmeow.MediaType `json:"MediaType_MediaType"`
	Size       int                 `json:"Size_int"`

	FileEncSha256 []byte `json:"FileEncSha256_arraybyte"`
	FileSha256    []byte `json:"FileSha256_arraybyte"`

	// Optional video thumbnail sidecar
	ThumbnailData       []byte `json:"ThumbnailData_arraybyte,omitempty"`
	ThumbnailTargetPath string `json:"ThumbnailTargetPath_string,omitempty"`
}

func marshalDownloadInfo(info DownloadInfo) ([]byte, error) {
	return json.Marshal(info)
}

func unmarshalDownloadInfo(fileID []byte) (DownloadInfo, error) {
	var info DownloadInfo
	err := json.Unmarshal(fileID, &info)
	return info, err
}

// AddThumbnailToFileId embeds JPEG thumbnail data into an existing fileId
// so DownloadFromFileId can save it as a sidecar alongside the media file.
func AddThumbnailToFileId(fileId string, thumbnailData []byte, thumbTargetPath string) string {
	if len(thumbnailData) == 0 || thumbTargetPath == "" {
		return fileId
	}
	info, err := unmarshalDownloadInfo([]byte(fileId))
	if err != nil {
		return fileId
	}
	info.ThumbnailData = thumbnailData
	info.ThumbnailTargetPath = thumbTargetPath
	bytes, err := marshalDownloadInfo(info)
	if err != nil {
		return fileId
	}
	return string(bytes)
}

func DownloadableMessageToFileId(client *whatsmeow.Client, msg whatsmeow.DownloadableMessage, targetPath string) string {
	var info DownloadInfo
	info.Version = downloadInfoVersion

	info.TargetPath = targetPath
	info.MediaKey = msg.GetMediaKey()
	info.Size = getSize(msg)
	info.FileEncSha256 = msg.GetFileEncSHA256()
	info.FileSha256 = msg.GetFileSHA256()
	info.DirectPath = msg.GetDirectPath()

	info.MediaType = whatsmeow.GetMediaType(msg)
	if len(info.MediaType) == 0 {
		return ""
	}

	bytes, err := marshalDownloadInfo(info)
	if err != nil {
		return ""
	}

	return string(bytes)
}

func FileIdToDownloadInfo(fileId string) (DownloadInfo, error) {
	info, err := unmarshalDownloadInfo([]byte(fileId))
	if err != nil {
		return info, err
	}
	if info.Version != downloadInfoVersion {
		return info, err
	}
	return info, nil
}
