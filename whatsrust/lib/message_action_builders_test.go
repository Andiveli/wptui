package main

import (
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestMessageActionBuildersOwnValidationAndBuilders(t *testing.T) {
	source, err := os.ReadFile("message_action_builders.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	for _, expected := range []string{
		"func newReactionRequest(",
		"func buildOrdinaryReaction(",
		"func buildOrdinaryEdit(",
		"func buildOrdinaryRevoke(",
		"func parseActionJID(",
	} {
		if !strings.Contains(body, expected) {
			t.Fatalf("action-builder seam missing %q", expected)
		}
	}

	actionSource, err := os.ReadFile("message_actions.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, removed := range []string{
		"func newReactionRequest(",
		"func buildOrdinaryReaction(",
		"func buildOrdinaryEdit(",
		"func buildOrdinaryRevoke(",
		"func parseActionJID(",
	} {
		if strings.Contains(string(actionSource), removed) {
			t.Fatalf("action-builder helper remains in message_actions.go: %q", removed)
		}
	}
}

func TestParseActionJID(t *testing.T) {
	tests := []struct {
		name string
		raw  string
		want types.JID
		ok   bool
	}{
		{name: "valid user", raw: "12345@s.whatsapp.net", want: types.NewJID("12345", types.DefaultUserServer), ok: true},
		{name: "empty", raw: "", ok: false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := parseActionJID(test.raw)
			if (err == nil) != test.ok {
				t.Fatalf("parseActionJID(%q) error = %v, want success = %v", test.raw, err, test.ok)
			}
			if test.ok && got != test.want {
				t.Fatalf("parseActionJID(%q) = %v, want %v", test.raw, got, test.want)
			}
		})
	}
}
