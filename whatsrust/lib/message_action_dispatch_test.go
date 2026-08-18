package main

import (
	"os"
	"strings"
	"testing"
)

func TestMessageActionDispatchOwnsPayloadInCUntilSynchronousCallbackReturns(t *testing.T) {
	source, err := os.ReadFile("message_action_dispatch.go")
	if err != nil {
		t.Fatal(err)
	}
	body, ok := extractFunctionBody(string(source), "func dispatchMessageActionEvent(action messageActionEvent)")
	if !ok {
		t.Fatal("dispatchMessageActionEvent function body not found in message_action_dispatch.go")
	}

	for _, fragment := range []string{
		"if eventHandler.callback == nil",
		"(*C.MessageActionEvent)(C.malloc(C.sizeof_MessageActionEvent))",
		"C.callMessageActionDispatchCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeMessageAction), data: unsafe.Pointer(payload)})",
		"C.free(unsafe.Pointer(payload))",
	} {
		if !strings.Contains(body, fragment) {
			t.Fatalf("message action dispatch must contain %q", fragment)
		}
	}

	callback := strings.Index(body, "C.callMessageActionDispatchCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeMessageAction), data: unsafe.Pointer(payload)})")
	freePayload := strings.Index(body, "C.free(unsafe.Pointer(payload))")
	if callback < 0 || freePayload < 0 || freePayload < callback {
		t.Fatal("message action payload must remain C-owned until the callback returns")
	}
}

func TestMessageActionDispatchLeavesMessageRouterInMain(t *testing.T) {
	source, err := os.ReadFile("event_wiring.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	if !strings.Contains(body, "dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage)") {
		t.Fatal("event wiring no longer delegates message action dispatch")
	}
	if strings.Contains(body, "func dispatchMessageActionEvent(action messageActionEvent)") {
		t.Fatal("message action dispatch remains in event_wiring.go")
	}
}
