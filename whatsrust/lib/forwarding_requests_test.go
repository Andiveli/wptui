package main

import (
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestNewForwardRequestValidationAndConversion(t *testing.T) {
	chat := types.NewJID("source", types.DefaultUserServer)
	sender := types.NewJID("sender", types.DefaultUserServer)
	newsletter := types.NewJID("channel", types.NewsletterServer)
	cases := []struct {
		name         string
		sourceChat   string
		sourceSender string
		sourceID     string
		destinations []string
		wantError    string
	}{
		{name: "valid request", sourceChat: chat.String(), sourceSender: sender.String(), sourceID: "message", destinations: []string{chat.String()}},
		{name: "broadcast source", sourceChat: types.StatusBroadcastJID.String(), sourceSender: sender.String(), sourceID: "message", destinations: []string{chat.String()}, wantError: "forward source chat is invalid"},
		{name: "newsletter source", sourceChat: newsletter.String(), sourceSender: sender.String(), sourceID: "message", destinations: []string{chat.String()}, wantError: "forward source chat is invalid"},
		{name: "empty source JID", sourceChat: "", sourceSender: sender.String(), sourceID: "message", destinations: []string{chat.String()}, wantError: "forward source chat is invalid"},
		{name: "missing sender", sourceChat: chat.String(), sourceSender: "", sourceID: "message", destinations: []string{chat.String()}, wantError: "forward requires source sender and message ID"},
		{name: "missing source ID", sourceChat: chat.String(), sourceSender: sender.String(), destinations: []string{chat.String()}, wantError: "forward requires source sender and message ID"},
		{name: "missing destinations", sourceChat: chat.String(), sourceSender: sender.String(), sourceID: "message", wantError: "forward requires at least one destination"},
		{name: "broadcast destination", sourceChat: chat.String(), sourceSender: sender.String(), sourceID: "message", destinations: []string{types.StatusBroadcastJID.String()}, wantError: "forward destination is invalid"},
		{name: "newsletter destination", sourceChat: chat.String(), sourceSender: sender.String(), sourceID: "message", destinations: []string{newsletter.String()}, wantError: "forward destination is invalid"},
		{name: "empty destination JID", sourceChat: chat.String(), sourceSender: sender.String(), sourceID: "message", destinations: []string{""}, wantError: "forward destination is invalid"},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			request, err := newForwardRequest(testCase.sourceChat, testCase.sourceSender, testCase.sourceID, testCase.destinations)
			if testCase.wantError != "" {
				if err == nil || err.Error() != testCase.wantError {
					t.Fatalf("error = %v, want %q", err, testCase.wantError)
				}
				return
			}
			if err != nil {
				t.Fatalf("newForwardRequest() error = %v", err)
			}
			if request.sourceChat != chat || request.sourceSender != sender || request.sourceID != types.MessageID("message") || len(request.destinations) != 1 || request.destinations[0] != chat {
				t.Fatalf("request conversion = %#v", request)
			}
		})
	}
}
