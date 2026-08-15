package main

import (
	"os"
	"strings"
	"testing"
)

func TestConnectionEventDispatchKeepsCallbackGuardAndKinds(t *testing.T) {
	source, err := os.ReadFile("connection_events.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	for _, expected := range []string{
		"func dispatchConnectedEvent()",
		"if eventHandler.callback == nil",
		"kind: C.uint8_t(EventTypeConnected)",
		"func emitLogoutResult(status uint8)",
		"kind: C.uint8_t(EventTypeLogoutResult)",
		"C.free(unsafe.Pointer(payload))",
	} {
		if !strings.Contains(body, expected) {
			t.Fatalf("connection event conversion missing %q", expected)
		}
	}
}

func TestConnectionEventDispatchLeavesMainHandlerAsRouter(t *testing.T) {
	source, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	body := string(source)
	if !strings.Contains(body, "dispatchConnectedEvent,") {
		t.Fatal("main handler does not delegate connected events")
	}
	if strings.Contains(body, "func emitLogoutResult(status uint8)") {
		t.Fatal("logout result conversion remains in main.go")
	}
}
