package main

import (
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

func TestResolveDMChatID(t *testing.T) {
	client := &whatsmeow.Client{}
	cases := []struct {
		name       string
		jidText    string
		want       string
		wantOK     bool
		wantLookup bool
	}{
		{name: "personal JID", jidText: "12345@s.whatsapp.net", want: "12345@s.whatsapp.net", wantOK: true, wantLookup: true},
		{name: "empty JID", jidText: "", wantOK: false},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			lookupCalled := false
			got, ok := resolveDMChatID(client, testCase.jidText, func(jid types.JID) string {
				lookupCalled = true
				return jid.String()
			})
			if got != testCase.want || ok != testCase.wantOK || lookupCalled != testCase.wantLookup {
				t.Fatalf("resolveDMChatID(%q) = (%q, %v), lookup called %v; want (%q, %v), lookup called %v", testCase.jidText, got, ok, lookupCalled, testCase.want, testCase.wantOK, testCase.wantLookup)
			}
		})
	}
}

func TestResolveDMChatIDRequiresClient(t *testing.T) {
	lookupCalled := false
	if got, ok := resolveDMChatID(nil, "12345@s.whatsapp.net", func(types.JID) string {
		lookupCalled = true
		return "unexpected"
	}); got != "" || ok || lookupCalled {
		t.Fatalf("nil client resolved DM chat as (%q, %v), lookup called %v", got, ok, lookupCalled)
	}
}
