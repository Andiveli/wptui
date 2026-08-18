package main

import (
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestForwardMessagesReportsBoundaryFailures(t *testing.T) {
	chat := types.NewJID("chat", types.DefaultUserServer).String()
	sender := types.NewJID("sender", types.DefaultUserServer).String()
	destinations := []string{
		types.NewJID("first", types.DefaultUserServer).String(),
		types.NewJID("second", types.DefaultUserServer).String(),
	}

	tests := []struct {
		name          string
		sourceChat    string
		sourceSender  string
		sourceID      string
		destinations  []string
		wantSucceeded uint32
		wantFailed    uint32
		wantFailure   uint8
	}{
		{
			name:         "invalid source request",
			sourceChat:   types.StatusBroadcastJID.String(),
			sourceSender: sender,
			sourceID:     "message",
			destinations: destinations,
			wantFailed:   2,
			wantFailure:  forwardFailureInvalidSource,
		},
		{
			name:         "unavailable client",
			sourceChat:   chat,
			sourceSender: sender,
			sourceID:     "message",
			destinations: destinations,
			wantFailed:   2,
			wantFailure:  forwardFailureSendFailed,
		},
	}

	for _, testCase := range tests {
		t.Run(testCase.name, func(t *testing.T) {
			got := forwardMessages(testCase.sourceID, testCase.sourceChat, testCase.sourceSender, false, testCase.destinations, nil)
			if got.succeeded != testCase.wantSucceeded || got.failed != testCase.wantFailed || got.failure != testCase.wantFailure {
				t.Fatalf("forwardMessages() = %#v, want succeeded=%d failed=%d failure=%d", got, testCase.wantSucceeded, testCase.wantFailed, testCase.wantFailure)
			}
		})
	}
}

func TestForwardingSourceBytesPreservesCBufferBoundaries(t *testing.T) {
	if got := forwardingSourceBytes(nil, 0); got != nil {
		t.Fatalf("empty source = %#v, want nil", got)
	}
}
