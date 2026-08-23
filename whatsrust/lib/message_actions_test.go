package main

import (
	"os"
	"strings"
	"testing"
)

func TestMessageActionBridgeExportsStayInActionModule(t *testing.T) {
	source, err := os.ReadFile("message_actions.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, fragment := range []string{"//export C_ReactToMessage", "//export C_EditMessage", "//export C_RevokeMessage"} {
		if !strings.Contains(string(source), fragment) {
			t.Fatalf("missing bridge export %q", fragment)
		}
	}
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, fragment := range []string{"//export C_ReactToMessage", "//export C_EditMessage", "//export C_RevokeMessage"} {
		if strings.Contains(string(mainSource), fragment) {
			t.Fatalf("action export still present in main.go: %q", fragment)
		}
	}
}
