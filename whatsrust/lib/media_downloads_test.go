package main

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

func TestDownloadFileReturnsFailureWhenClientIsUnavailable(t *testing.T) {
	fileID, err := json.Marshal(DownloadInfo{Version: downloadInfoVersion, TargetPath: "media.jpg"})
	if err != nil {
		t.Fatal(err)
	}
	previous := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previous) })
	lifecycleState.publishClient(nil)

	root := t.TempDir()
	clientSnapshot := lifecycleState.clientSnapshot()
	if got := downloadFile(clientSnapshot, string(fileID), root); got != FileStatusDownloadFailed {
		t.Fatalf("download status = %d, want failure", got)
	}
}

func TestMediaDownloadExportsAndPathHelpersStayOutOfMain(t *testing.T) {
	mediaSource, err := os.ReadFile("media_downloads.go")
	if err != nil {
		t.Fatal(err)
	}
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, fragment := range []string{"func C_DownloadFile", "func SliceIndex", "func ExtensionByType"} {
		if !strings.Contains(string(mediaSource), fragment) {
			t.Fatalf("media download module must contain %q", fragment)
		}
		if strings.Contains(string(mainSource), fragment) {
			t.Fatalf("main.go must not contain %q", fragment)
		}
	}
	for _, fragment := range []string{"func DownloadFromFileId", "func safeDownloadTarget", "func writeDownload"} {
		if strings.Contains(string(mainSource), fragment) {
			t.Fatalf("main.go must not contain %q", fragment)
		}
	}
}

func TestExtensionByTypeUsesDefaultsAndStableCommonExtensions(t *testing.T) {
	for _, testCase := range []struct {
		name, mimeType, defaultExt, want string
	}{
		{name: "unknown MIME uses default", mimeType: "application/x-unknown", defaultExt: ".bin", want: ".bin"},
		{name: "JPEG prefers jpg", mimeType: "image/jpeg", defaultExt: ".bin", want: ".jpg"},
		{name: "PNG preserves MIME extension", mimeType: "image/png", defaultExt: ".bin", want: ".png"},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			if got := ExtensionByType(testCase.mimeType, testCase.defaultExt); got != testCase.want {
				t.Fatalf("extension = %q, want %q", got, testCase.want)
			}
		})
	}
}
