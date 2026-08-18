package main

import (
	"os"
	"strings"
	"testing"
)

func TestJIDConversionOwnsBridgeHelpers(t *testing.T) {
	source, err := os.ReadFile("jid_conversion.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	for _, expected := range []string{
		"func jidToC(jid types.JID) C.JID",
		"func cToJid(cjid C.JID) types.JID",
		"jid.ToNonAD().String()",
		"types.ParseJID(C.GoString(cjid))",
	} {
		if !strings.Contains(body, expected) {
			t.Fatalf("JID conversion seam missing %q", expected)
		}
	}
}

func TestJIDConversionLeavesMainFreeOfHelpers(t *testing.T) {
	source, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	for _, removed := range []string{
		"func jidToC(jid types.JID) C.JID",
		"func cToJid(cjid C.JID) types.JID",
	} {
		if strings.Contains(body, removed) {
			t.Fatalf("JID conversion helper remains in main.go: %q", removed)
		}
	}
}
