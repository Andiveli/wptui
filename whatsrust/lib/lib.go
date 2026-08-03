package main

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"

	"go.mau.fi/whatsmeow"
	"golang.org/x/sys/unix"
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

// AddThumbnailToFileId embeds JPEG thumbnail data into an existing fileId
// so DownloadFromFileId can save it as a sidecar alongside the media file.
func AddThumbnailToFileId(fileId string, thumbnailData []byte, thumbTargetPath string) string {
	if len(thumbnailData) == 0 || thumbTargetPath == "" {
		return fileId
	}
	var info DownloadInfo
	if err := json.Unmarshal([]byte(fileId), &info); err != nil {
		return fileId
	}
	info.ThumbnailData = thumbnailData
	info.ThumbnailTargetPath = thumbTargetPath
	bytes, err := json.Marshal(info)
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

	bytes, err := json.Marshal(info)
	if err != nil {
		return ""
	}

	str := string(bytes)

	return str
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
func FileIdToDownloadInfo(fileId string) (DownloadInfo, error) {
	var info DownloadInfo
	err := json.Unmarshal([]byte(fileId), &info)
	if err != nil {
		return info, err
	}
	if info.Version != downloadInfoVersion {
		return info, err
	}
	return info, nil
}
func DownloadFromFileId(client *whatsmeow.Client, fileId string, basePath string) int {
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

func safeDownloadTarget(basePath, targetPath string) (string, error) {
	clean := filepath.Clean(targetPath)
	if targetPath == "" || filepath.IsAbs(targetPath) || clean == "." || clean != targetPath || clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
		return "", errors.New("unsafe download target path")
	}
	fd, err := openDownloadRoot(basePath)
	if err != nil {
		return "", err
	}
	unix.Close(fd)
	return filepath.Join(basePath, clean), nil
}

func writeDownload(basePath, targetPath string, data []byte) error {
	return writeDownloadWithWriter(basePath, targetPath, data, func(file *os.File, data []byte) (int, error) {
		return file.Write(data)
	})
}

func writeDownloadWithWriter(basePath, targetPath string, data []byte, write func(*os.File, []byte) (int, error)) error {
	if _, err := safeDownloadTarget(basePath, targetPath); err != nil {
		return err
	}
	parent, err := openDownloadRoot(basePath)
	if err != nil {
		return err
	}
	defer func() {
		unix.Close(parent)
	}()
	parts := strings.Split(targetPath, string(filepath.Separator))
	for _, part := range parts[:len(parts)-1] {
		next, err := unix.Openat(parent, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		if err == unix.ENOENT {
			err = unix.Mkdirat(parent, part, 0o700)
			next, err = unix.Openat(parent, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		}
		if err != nil {
			return err
		}
		unix.Close(parent)
		parent = next
	}
	tempName := "." + parts[len(parts)-1] + ".part"
	fd, err := unix.Openat(parent, tempName, unix.O_WRONLY|unix.O_CREAT|unix.O_EXCL|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0o600)
	if err != nil {
		return err
	}
	tempCreated := true
	defer func() {
		if tempCreated {
			_ = unix.Unlinkat(parent, tempName, 0)
		}
	}()
	file := os.NewFile(uintptr(fd), tempName)
	_, err = write(file, data)
	if closeErr := file.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	if err := unix.Renameat2(parent, tempName, parent, parts[len(parts)-1], unix.RENAME_NOREPLACE); err != nil {
		return err
	}
	tempCreated = false
	return nil
}

func openDownloadRoot(basePath string) (int, error) {
	path, err := filepath.Abs(basePath)
	if err != nil {
		return -1, err
	}
	fd, err := unix.Open("/", unix.O_RDONLY|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
	if err != nil {
		return -1, err
	}
	for _, part := range strings.Split(path, string(filepath.Separator)) {
		if part == "" {
			continue
		}
		next, err := unix.Openat(fd, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		if err == unix.ENOENT {
			err = unix.Mkdirat(fd, part, 0o700)
			next, err = unix.Openat(fd, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		}
		unix.Close(fd)
		if err != nil {
			return -1, err
		}
		fd = next
	}
	return fd, nil
}
