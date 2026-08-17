package main

import (
	"os"
	"strings"
	"testing"
)

func TestIncomingMessageActionClassifierOwnsNormalizedEditSeam(t *testing.T) {
	seam, err := os.ReadFile("message_action_incoming.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(seam), "func messageActionEventFromIncomingMessage") {
		t.Fatal("incoming message action seam is missing its classifier")
	}

	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(mainSource), "func messageActionEventFromIncomingMessage") {
		t.Fatal("incoming message action classifier remains in main.go")
	}
}
