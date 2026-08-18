package main

import (
	"os"
	"strings"
	"testing"
)

func TestPresenceFFIOwnsTestPresenceEmitters(t *testing.T) {
	seam, err := os.ReadFile("presence_ffi.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, export := range []string{
		"//export C_TestEmitPresenceEvent",
		"//export C_TestEmitPresenceEventsConcurrently",
		"callPresenceTestHandler",
	} {
		if !strings.Contains(string(seam), export) {
			t.Fatalf("presence FFI seam is missing %s", export)
		}
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, export := range []string{
		"func C_TestEmitPresenceEvent",
		"func C_TestEmitPresenceEventsConcurrently",
		"callPresenceTestHandler",
	} {
		if strings.Contains(string(mainSource), export) {
			t.Fatalf("presence FFI export remains in main.go: %s", export)
		}
	}
}
