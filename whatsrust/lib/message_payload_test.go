package main

import (
	"os"
	"strings"
	"testing"
)

func TestHandleMessageOwnsTextPayloadEmission(t *testing.T) {
	source, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	mainSource := string(source)

	if !strings.Contains(mainSource, "func emitTextMessage(") {
		t.Fatal("text payload emission seam is missing")
	}
	if strings.Count(mainSource, "emitTextMessage(cinfo,") != 2 {
		t.Fatal("conversation and extended-text payloads must use the text seam")
	}
	for _, fragment := range []string{
		"if msg.ImageMessage != nil",
		"if msg.VideoMessage != nil",
		"if msg.AudioMessage != nil",
		"if msg.DocumentMessage != nil",
		"if msg.StickerMessage != nil",
	} {
		if !strings.Contains(mainSource, fragment) {
			t.Fatalf("remaining media payload responsibility changed: %q", fragment)
		}
	}
}
