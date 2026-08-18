package main

import (
	"testing"

	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
)

func TestMessageActionProbeTraversesNestedWrappers(t *testing.T) {
	message := &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
		EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
			ProtocolMessage: &waE2E.ProtocolMessage{
				Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
				Key:  &waCommon.MessageKey{ID: stringPointer("target")},
			},
		}},
	}}}

	probe := messageActionProbeFromMessage(message, "message")

	if !probe.present || !probe.ephemeral || !probe.edited || !probe.futureProof {
		t.Fatalf("probe = %#v", probe)
	}
	if !probe.hasActionProtocol() || probe.protocolPath != "message.ephemeral.edited.protocol" {
		t.Fatalf("protocol = %#v, path = %q", probe.protocol, probe.protocolPath)
	}
}

func TestMessageActionProbeReportsAbsentMessages(t *testing.T) {
	probe := messageActionProbeFromMessage(nil, "raw")

	if probe.present || probe.protocol != nil || probe.secretEncrypted != nil {
		t.Fatalf("probe = %#v", probe)
	}
}
