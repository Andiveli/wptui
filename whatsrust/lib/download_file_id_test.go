package main

import (
	"bytes"
	"testing"

	"go.mau.fi/whatsmeow"
)

func TestDownloadInfoEncodingPreservesWireMetadata(t *testing.T) {
	want := DownloadInfo{
		Version:             downloadInfoVersion,
		DirectPath:          "/v/t62/media",
		TargetPath:          "videos/message.mp4",
		MediaKey:            []byte("media-key"),
		MediaType:           whatsmeow.MediaVideo,
		Size:                42,
		FileEncSha256:       []byte("encrypted-hash"),
		FileSha256:          []byte("plain-hash"),
		ThumbnailData:       []byte("thumbnail"),
		ThumbnailTargetPath: "videos/message.jpg",
	}

	encoded, err := marshalDownloadInfo(want)
	if err != nil {
		t.Fatal(err)
	}
	got, err := unmarshalDownloadInfo(encoded)
	if err != nil {
		t.Fatal(err)
	}
	if got.Version != want.Version || got.DirectPath != want.DirectPath || got.TargetPath != want.TargetPath || got.MediaType != want.MediaType || got.Size != want.Size || got.ThumbnailTargetPath != want.ThumbnailTargetPath || !bytes.Equal(got.MediaKey, want.MediaKey) || !bytes.Equal(got.FileEncSha256, want.FileEncSha256) || !bytes.Equal(got.FileSha256, want.FileSha256) || !bytes.Equal(got.ThumbnailData, want.ThumbnailData) {
		t.Fatalf("decoded metadata = %#v, want %#v", got, want)
	}
}

func TestAddThumbnailToFileIdLeavesInvalidOrEmptyInputsUntouched(t *testing.T) {
	for _, fileID := range []string{"not-json", `{"Version_int":1}`} {
		if got := AddThumbnailToFileId(fileID, nil, "video.jpg"); got != fileID {
			t.Fatalf("file ID %q changed to %q", fileID, got)
		}
	}
	if got := AddThumbnailToFileId("not-json", []byte("thumbnail"), "video.jpg"); got != "not-json" {
		t.Fatalf("invalid file ID changed to %q", got)
	}
}

func TestAddThumbnailToFileIdEmbedsSidecarMetadata(t *testing.T) {
	fileID, err := marshalDownloadInfo(DownloadInfo{Version: downloadInfoVersion, TargetPath: "video.mp4"})
	if err != nil {
		t.Fatal(err)
	}

	withThumbnail := AddThumbnailToFileId(string(fileID), []byte("thumbnail"), "video.jpg")
	got, err := FileIdToDownloadInfo(withThumbnail)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got.ThumbnailData, []byte("thumbnail")) || got.ThumbnailTargetPath != "video.jpg" {
		t.Fatalf("thumbnail metadata = %#v", got)
	}
}
