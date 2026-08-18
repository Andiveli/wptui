package main

import (
	"fmt"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestRawPresenceDiagnosticsDisabledIsNoOp(t *testing.T) {
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(false)
	diagnostics.record(&events.Presence{From: types.NewJID("private-user", types.DefaultUserServer)})
	if report := diagnostics.drain(); report != "" {
		t.Fatalf("disabled diagnostics returned %q", report)
	}
}

func TestRawPresenceDiagnosticsAreBoundedOrderedAndRedacted(t *testing.T) {
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	for index := 0; index <= maxRawPresenceDiagnosticEntries; index++ {
		diagnostics.record(&events.Presence{
			From:        types.NewJID(fmt.Sprintf("private-user-%d", index), types.DefaultUserServer),
			Unavailable: index%2 == 0,
			LastSeen:    time.Unix(int64(index+1), 0),
		})
	}

	report := diagnostics.drain()
	if strings.Contains(report, "private-user") {
		t.Fatal("raw presence report exposed JID user data")
	}
	if !strings.Contains(report, "raw presence events received: 51\n") || strings.Contains(report, "\n1. server=") {
		t.Fatalf("unexpected bounded report header/order:\n%s", report)
	}
	if !strings.Contains(report, "2. server=s.whatsapp.net, unavailable=false, last_seen_present=true") ||
		!strings.Contains(report, "51. server=s.whatsapp.net, unavailable=true, last_seen_present=true") {
		t.Fatalf("unexpected retained event order:\n%s", report)
	}
}

func TestRawPresenceDiagnosticsClassifyServersAndResetOnDrain(t *testing.T) {
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	for _, event := range []*events.Presence{
		{From: types.NewJID("secret-a", types.DefaultUserServer)},
		{From: types.NewJID("secret-b", types.HiddenUserServer), Unavailable: true},
		{From: types.NewJID("secret-c", types.GroupServer)},
	} {
		diagnostics.record(event)
	}

	report := diagnostics.drain()
	for _, expected := range []string{"server=s.whatsapp.net", "server=lid", "server=other"} {
		if !strings.Contains(report, expected) {
			t.Fatalf("report missing %q:\n%s", expected, report)
		}
	}
	if strings.Contains(report, "secret-") || !strings.Contains(report, "last_seen_present=false") {
		t.Fatalf("report leaked identity or omitted safe metadata:\n%s", report)
	}
	if report := diagnostics.drain(); report != "raw presence events received: 0\n" {
		t.Fatalf("drain did not reset run data: %q", report)
	}
	diagnostics.reset(true)
	if report := diagnostics.drain(); report != "raw presence events received: 0\n" {
		t.Fatalf("run reset retained data: %q", report)
	}
}
