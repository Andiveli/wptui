package main

import (
	"os"
	"strings"
	"testing"
)

func TestChatSettingsFFIOwnsChatSettingsExport(t *testing.T) {
	seam, err := os.ReadFile("chat_settings_ffi.go")
	if err != nil {
		t.Fatal(err)
	}
	seamSource := string(seam)
	if !strings.Contains(seamSource, "//export C_GetChatSettings") ||
		!strings.Contains(seamSource, "func C_GetChatSettings(cjid C.JID)") {
		t.Fatal("chat settings FFI seam is missing C_GetChatSettings")
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "func C_GetChatSettings") {
		t.Fatal("chat settings FFI export remains in main.go")
	}
}

func TestChatSettingsPayloadToCPreservesABIFields(t *testing.T) {
	result := chatSettingsPayloadToC(chatSettingsPayload{
		found:      true,
		mutedUntil: 123,
		pinned:     true,
		archived:   true,
	})
	if !bool(result.found) || result.muted_until != 123 || !bool(result.pinned) || !bool(result.archived) {
		t.Fatalf("C result = %#v, want all chat settings fields preserved", result)
	}
}
