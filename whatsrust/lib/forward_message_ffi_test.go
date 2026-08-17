package main

import (
	"os"
	"strings"
	"testing"
)

func TestForwardMessageFFIOwnsForwardMessageExport(t *testing.T) {
	seam, err := os.ReadFile("forward_message_ffi.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(seam), "//export C_ForwardMessage") {
		t.Fatal("forward-message FFI seam is missing C_ForwardMessage")
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "func C_ForwardMessage") {
		t.Fatal("forward-message FFI export remains in main.go")
	}
}
