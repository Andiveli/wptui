package main

import (
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func resetEventCensusForTest() {
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	eventCensus.nextSeq = 0
	eventCensus.entries = nil
}

func TestMessageActionEventCensusIsDisabledWithoutDebug(t *testing.T) {
	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "")
	resetEventCensusForTest()
	messageActionCensusDiagnostic(&events.Receipt{Type: types.ReceiptTypeRead})
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	if len(eventCensus.entries) != 0 {
		t.Fatalf("disabled census recorded %d entries", len(eventCensus.entries))
	}
}

func TestMessageActionEventCensusIsBoundedAndOrdered(t *testing.T) {
	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "1")
	resetEventCensusForTest()
	for range messageActionCensusLimit + 1 {
		messageActionCensusDiagnostic(&events.Receipt{Type: types.ReceiptTypeRead})
	}
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	if len(eventCensus.entries) != messageActionCensusLimit {
		t.Fatalf("census length = %d, want %d", len(eventCensus.entries), messageActionCensusLimit)
	}
	if !strings.HasPrefix(eventCensus.entries[0], "census=event seq=2 ") || !strings.HasPrefix(eventCensus.entries[len(eventCensus.entries)-1], "census=event seq=101 ") {
		t.Fatalf("census order = first %q, last %q", eventCensus.entries[0], eventCensus.entries[len(eventCensus.entries)-1])
	}
}
