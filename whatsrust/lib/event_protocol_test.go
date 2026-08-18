package main

import (
	"os"
	"strings"
	"testing"
)

func TestEventTypeConstantsAreOwnedByEventProtocol(t *testing.T) {
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "EventType") {
		t.Fatal("main.go must not own EventType protocol constants")
	}
	protocolSource, err := os.ReadFile("event_protocol.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, constant := range []string{
		"EventTypeSyncProgress",
		"EventTypeAppStateSyncComplete",
		"EventTypeReceipt",
		"EventTypeReaction",
		"EventTypeConnected",
		"EventTypeMessageAction",
		"EventTypeChat",
		"EventTypeLogoutResult",
	} {
		if !strings.Contains(string(protocolSource), constant) {
			t.Fatalf("event_protocol.go must own %s", constant)
		}
	}
}

func TestEventTypeValuesRemainStable(t *testing.T) {
	for _, tt := range []struct {
		name string
		got  int
		want int
	}{
		{"sync progress", EventTypeSyncProgress, 0},
		{"app state sync complete", EventTypeAppStateSyncComplete, 1},
		{"receipt", EventTypeReceipt, 2},
		{"reaction", EventTypeReaction, 3},
		{"connected", EventTypeConnected, 5},
		{"message action", EventTypeMessageAction, 6},
		{"chat", EventTypeChat, 7},
		{"logout result", EventTypeLogoutResult, 8},
	} {
		t.Run(tt.name, func(t *testing.T) {
			if tt.got != tt.want {
				t.Fatalf("value = %d, want %d", tt.got, tt.want)
			}
		})
	}
}
