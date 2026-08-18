package main

import (
	"os"
	"strings"
	"testing"
)

func TestTextPayloadOwnershipIsKeptWithEmitter(t *testing.T) {
	mainSource := string(mustRead(t, "main.go"))
	source, err := os.ReadFile("text_message_payload.go")
	if err != nil {
		t.Fatal(err)
	}
	textSource := string(source)

	if strings.Contains(mainSource, "func emitTextMessage(") {
		t.Fatal("text payload emission must not remain in HandleMessage's composition file")
	}
	for _, fragment := range []string{
		"typedef struct {\n\tchar* text;\n} TextMessage;",
		"func emitTextMessage(",
		"C.sizeof_TextMessage",
		"defer C.free(unsafe.Pointer(ctext))",
		"defer C.free(unsafe.Pointer(content))",
	} {
		if !strings.Contains(textSource, fragment) {
			t.Fatalf("text payload ownership is missing: %q", fragment)
		}
	}
	if strings.Count(mainSource, "emitTextMessage(cinfo,") != 2 {
		t.Fatal("conversation and extended-text payloads must use the text seam")
	}
}
