package main

import (
	"os"
	"strings"
	"testing"
)

func TestContactsFFIOwnsContactsExport(t *testing.T) {
	seam, err := os.ReadFile("contacts.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(seam), "func C_GetContacts()") {
		t.Fatal("contacts FFI seam is missing C_GetContacts")
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "func C_GetContacts()") {
		t.Fatal("contacts FFI export remains in main.go")
	}
}
