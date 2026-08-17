package main

import (
	"os"
	"strings"
	"testing"
)

func TestCallbackAndLogRegistrationStaysDedicated(t *testing.T) {
	registration, err := os.ReadFile("callback_log_registration.go")
	if err != nil {
		t.Fatal(err)
	}
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}

	for _, expected := range []string{
		"func LOG_LEVEL",
		"func LOG_ERROR",
		"func LOG_WARN",
		"func LOG_INFO",
		"func LOG_DEBUG",
		"func C_SetLogHandler",
		"func C_SetEventHandler",
		"func C_SetMessageHandler",
		"func C_SetPresenceHandler",
	} {
		if !strings.Contains(string(registration), expected) {
			t.Fatalf("callback/log registration missing %q", expected)
		}
	}
	if strings.Contains(string(mainSource), "func C_SetEventHandler") {
		t.Fatal("event callback registration remains in main.go")
	}
	if !strings.Contains(string(mainSource), "func AddEventHandlers") {
		t.Fatal("event wiring was moved with callback registration")
	}
}
