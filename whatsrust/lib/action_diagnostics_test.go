package main

import (
	"os"
	"strings"
	"testing"
)

func TestActionDiagnosticsOwnDebugEmission(t *testing.T) {
	source, err := os.ReadFile("action_diagnostics.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	for _, expected := range []string{
		"func messageActionDiagnostic(",
		"func emitStatusProtocolDiagnostic(",
		"WPTUI_MESSAGE_ACTION_DEBUG",
	} {
		if !strings.Contains(body, expected) {
			t.Fatalf("action-diagnostics seam missing %q", expected)
		}
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, removed := range []string{
		"func messageActionDiagnostic(",
		"func emitStatusProtocolDiagnostic(",
	} {
		if strings.Contains(string(mainSource), removed) {
			t.Fatalf("action-diagnostics helper remains in main.go: %q", removed)
		}
	}
}
