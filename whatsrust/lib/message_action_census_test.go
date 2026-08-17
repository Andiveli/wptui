package main

import (
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestMessageActionCensusFormattingRedactsAndClassifies(t *testing.T) {
	chat := types.NewJID("chat-secret", types.DefaultUserServer)
	sender := types.NewJID("sender-secret", types.DefaultUserServer)
	event := &events.Message{
		Info:       types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: sender}, ID: "message-secret"},
		RawMessage: &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")},
		Message:    &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")},
	}
	line := messageActionCensusLine(event)
	for _, secret := range []string{"DO-NOT-LOG-BODY", "message-secret", chat.String(), sender.String()} {
		if strings.Contains(line, secret) {
			t.Fatalf("census leaked %q: %s", secret, line)
		}
	}
	for _, field := range []string{"event_type=events_message", "raw_kinds=conversation", "message_kinds=conversation", "info_id=<id:", "wrappers=raw"} {
		if !strings.Contains(line, field) {
			t.Fatalf("census omitted %q: %s", field, line)
		}
	}
}

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

func TestMessageActionCensusClassifiesEventSubtypes(t *testing.T) {
	cases := []struct {
		name  string
		event any
		want  string
	}{
		{name: "receipt", event: &events.Receipt{Type: types.ReceiptTypeRead}, want: "event_type=events_receipt subtype=receipt_read"},
		{name: "unknown", event: &events.Connected{}, want: "event_type=events_connected"},
		{name: "app state sync", event: &events.AppStateSyncComplete{Name: "critical_block"}, want: "subtype=sync_complete app_state=critical_block"},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			if got := messageActionCensusLine(testCase.event); !strings.Contains(got, testCase.want) {
				t.Fatalf("census = %s, want %q", got, testCase.want)
			}
		})
	}
}

func TestSafeCensusNameNormalizesAndRedactsCharacters(t *testing.T) {
	if got, want := safeCensusName("Message.Edit/Status-2"), "message_editstatus_2"; got != want {
		t.Fatalf("safe census name = %q, want %q", got, want)
	}
}
