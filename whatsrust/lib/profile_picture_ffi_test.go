package main

import (
	"os"
	"strings"
	"testing"
)

func TestProfilePictureFFIOwnsProfilePictureExports(t *testing.T) {
	seam, err := os.ReadFile("profile_picture.go")
	if err != nil {
		t.Fatal(err)
	}
	seamSource := string(seam)
	for _, export := range []string{
		"func C_GetProfilePicture(jid *C.char, isCommunity C.bool, commonGID *C.char)",
		"func C_FreeProfilePicture(result C.ProfilePictureResult)",
	} {
		if !strings.Contains(seamSource, export) {
			t.Fatalf("profile picture FFI seam is missing %s", export)
		}
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "func C_GetProfilePicture") ||
		strings.Contains(string(mainSource), "func C_FreeProfilePicture") {
		t.Fatal("profile picture FFI exports remain in main.go")
	}
}
