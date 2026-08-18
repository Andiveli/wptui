package main

import (
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestJidsMatchSelfNormalizesDeviceIdentity(t *testing.T) {
	self := types.NewADJID("self", 0, 7)
	candidate := types.NewJID("self", types.DefaultUserServer)
	if !jidsMatchSelf(self, candidate) {
		t.Fatal("device and user JIDs should identify the same participant")
	}
	if jidsMatchSelf(self, types.NewJID("other", types.DefaultUserServer)) {
		t.Fatal("different participants must not match")
	}
}
