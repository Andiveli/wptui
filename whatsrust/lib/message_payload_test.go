package main

import (
	"os"
	"strings"
	"testing"
)

func TestHandleMessageDelegatesFilePayloadEmission(t *testing.T) {
	if strings.Contains(string(mustRead(t, "main.go")), "func emitFileMessage(") {
		t.Fatal("file payload emission must remain outside HandleMessage's composition file")
	}
	source := string(mustRead(t, "file_message_payload.go"))
	for _, fragment := range []string{
		"func emitFileMessage(",
		"func emitImageMessage(",
		"func emitVideoMessage(",
		"func emitAudioMessage(",
		"func emitDocumentMessage(",
		"func emitStickerMessage(",
	} {
		if !strings.Contains(source, fragment) {
			t.Fatalf("file payload seam is missing: %q", fragment)
		}
	}
}

func TestFileMessageMentionRangesAreEmptyWhenNoRangesExist(t *testing.T) {
	cases := []struct {
		name  string
		input []mentionRange
	}{
		{name: "empty caption", input: nil},
		{name: "caption without mention", input: nil},
		{name: "audio", input: nil},
		{name: "sticker", input: nil},
		{name: "ordinary file", input: nil},
	}

	for _, tt := range cases {
		t.Run(tt.name, func(t *testing.T) {
			memory, mentionRanges, mentionRangeCount := buildFileMessageMentionRanges(tt.input)
			if memory != nil {
				t.Fatal("empty ranges must not allocate mention range memory")
			}
			if mentionRanges != nil {
				t.Fatalf("mentionRanges = %p, want nil", mentionRanges)
			}
			if mentionRangeCount != 0 {
				t.Fatalf("mentionRangeCount = %d, want 0", mentionRangeCount)
			}
		})
	}
}

func mustRead(t *testing.T, name string) []byte {
	t.Helper()
	source, err := os.ReadFile(name)
	if err != nil {
		t.Fatal(err)
	}
	return source
}
