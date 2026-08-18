package main

import (
	"os"
	"strings"
	"testing"
)

func TestEventWiringOwnsRegistrationAndDispatchCases(t *testing.T) {
	source, err := os.ReadFile("event_wiring.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	for _, expected := range []string{
		"func AddEventHandlers()",
		"client.AddEventHandler(func(rawEvt any)",
		"case *events.Connected:",
		"case *events.Presence:",
		"case *events.AppStateSyncComplete:",
		"case *events.Message:",
		"case *events.Receipt:",
		"case *events.HistorySync:",
		"dispatchHistorySync(",
	} {
		if !strings.Contains(body, expected) {
			t.Fatalf("event wiring missing %q", expected)
		}
	}
}

func TestEventWiringLeavesMainFreeOfRegistration(t *testing.T) {
	source, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(source), "func AddEventHandlers()") {
		t.Fatal("event registration remains in main.go")
	}
}
