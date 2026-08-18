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

func mustRead(t *testing.T, name string) []byte {
	t.Helper()
	source, err := os.ReadFile(name)
	if err != nil {
		t.Fatal(err)
	}
	return source
}
