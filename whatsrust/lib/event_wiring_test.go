package main

import (
	"os"
	"reflect"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestUnavailableViewOnceUndecryptableEventDispatchesSingleSafePlaceholder(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:   types.NewJID("chat", types.DefaultUserServer),
		Sender: types.NewJID("sender", types.DefaultUserServer),
	}, ID: "view-once-id"}
	event := &events.UndecryptableMessage{
		Info:            info,
		IsUnavailable:   true,
		UnavailableType: events.UnavailableTypeViewOnce,
	}

	var dispatched []types.MessageInfo
	dispatchUndecryptableMessage(event, func(gotInfo types.MessageInfo, _ bool) {
		dispatched = append(dispatched, gotInfo)
	})

	if len(dispatched) != 1 {
		t.Fatalf("dispatch count = %d, want exactly one", len(dispatched))
	}
	if !reflect.DeepEqual(dispatched[0], info) {
		t.Fatalf("message info was not preserved: got %#v, want %#v", dispatched[0], info)
	}
}

func TestUndecryptableMessageDispatchIgnoresUnrelatedEvents(t *testing.T) {
	info := types.MessageInfo{ID: "undecryptable-id"}
	for name, event := range map[string]*events.UndecryptableMessage{
		"unavailable ordinary message": {
			Info:          info,
			IsUnavailable: true,
		},
		"decrypt failure view-once": {
			Info:            info,
			UnavailableType: events.UnavailableTypeViewOnce,
		},
	} {
		t.Run(name, func(t *testing.T) {
			dispatched := 0
			dispatchUndecryptableMessage(event, func(types.MessageInfo, bool) {
				dispatched++
			})
			if dispatched != 0 {
				t.Fatalf("dispatch count = %d, want zero", dispatched)
			}
		})
	}
}

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
		"case *events.UndecryptableMessage:",
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
