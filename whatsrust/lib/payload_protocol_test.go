package main

import (
	"os"
	"strings"
	"testing"
)

func TestPayloadProtocolConstantsAreOwnedByPayloadProtocol(t *testing.T) {
	payloadSource, err := os.ReadFile("payload_protocol.go")
	if err != nil {
		t.Fatal(err)
	}
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, constant := range []string{
		"MessageTypeText",
		"MessageTypeFile",
		"FileTypeImage",
		"FileTypeVideo",
		"FileTypeAudio",
		"FileTypeDocument",
		"FileTypeSticker",
	} {
		if !strings.Contains(string(payloadSource), constant) {
			t.Fatalf("payload_protocol.go must own %s", constant)
		}
		if strings.Contains(string(mainSource), constant) {
			t.Fatalf("main.go must not own %s", constant)
		}
	}
}

func TestPayloadProtocolValuesRemainStable(t *testing.T) {
	for _, tt := range []struct {
		name string
		got  int
		want int
	}{
		{"message text", MessageTypeText, 0},
		{"message file", MessageTypeFile, 1},
		{"file image", FileTypeImage, 0},
		{"file video", FileTypeVideo, 1},
		{"file audio", FileTypeAudio, 2},
		{"file document", FileTypeDocument, 3},
		{"file sticker", FileTypeSticker, 4},
	} {
		t.Run(tt.name, func(t *testing.T) {
			if tt.got != tt.want {
				t.Fatalf("value = %d, want %d", tt.got, tt.want)
			}
		})
	}
}
