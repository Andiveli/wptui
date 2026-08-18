package main

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

func TestPinnedPresenceContractAndNarrowSubscription(t *testing.T) {
	jid := types.NewJID("12345", types.DefaultUserServer)
	calls := 0
	subscribe := func(_ context.Context, got types.JID) error {
		calls++
		if got != jid {
			t.Fatalf("subscribed to %s, want %s", got, jid)
		}
		return nil
	}
	if got := subscribePresence(jid, subscribe); got != subscribePresenceAccepted || calls != 1 {
		t.Fatalf("individual subscription result=%d calls=%d", got, calls)
	}
	if got := subscribePresence(types.NewJID("group", types.GroupServer), subscribe); got != subscribePresenceRejected || calls != 1 {
		t.Fatalf("group subscription must be rejected before whatsmeow, calls=%d", calls)
	}
}

func TestSubscribePresenceResultIdentifiesNoPrivacyToken(t *testing.T) {
	jid := types.NewJID("12345", types.DefaultUserServer)
	tests := []struct {
		name string
		err  error
		want uint8
	}{
		{name: "valid token accepted", want: subscribePresenceAccepted},
		{name: "missing token identifiable", err: fmt.Errorf("wrapped: %w", whatsmeow.ErrNoPrivacyToken), want: subscribePresenceNoPrivacyToken},
		{name: "transport error rejected", err: errors.New("transport failed"), want: subscribePresenceRejected},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := subscribePresence(jid, func(context.Context, types.JID) error { return tt.err })
			if got != tt.want {
				t.Fatalf("subscribePresence() = %d, want %d", got, tt.want)
			}
		})
	}
}

func TestPresenceSubscriptionsRequirePrivacyToken(t *testing.T) {
	client := &whatsmeow.Client{}
	configurePresenceSubscriptions(client)
	if !client.ErrorOnSubscribePresenceWithoutToken {
		t.Fatal("presence subscriptions must reject missing privacy tokens")
	}
}

func TestConnectedAnnouncesPresenceBeforeReportingReady(t *testing.T) {
	var calls []string

	ready := handleConnected(
		func(_ context.Context, presence types.Presence) error {
			if presence != types.PresenceAvailable {
				t.Fatalf("presence = %q, want available", presence)
			}
			calls = append(calls, "announce")
			return nil
		},
		func() { calls = append(calls, "connected") },
		func(string, ...any) { t.Fatal("unexpected warning") },
	)

	if !ready {
		t.Fatal("successful announcement must report readiness")
	}
	if got, want := strings.Join(calls, ","), "announce,connected"; got != want {
		t.Fatalf("call order = %q, want %q", got, want)
	}
}

func TestConnectedRepeatsPresenceAnnouncementAfterReconnect(t *testing.T) {
	var calls []string
	announce := func(_ context.Context, _ types.Presence) error {
		calls = append(calls, "announce")
		return nil
	}
	connected := func() { calls = append(calls, "connected") }
	warn := func(string, ...any) { t.Fatal("unexpected warning") }

	handleConnected(announce, connected, warn)
	handleConnected(announce, connected, warn)

	if got, want := strings.Join(calls, ","), "announce,connected,announce,connected"; got != want {
		t.Fatalf("reconnect call order = %q, want %q", got, want)
	}
}

func TestConnectedAnnouncementFailureDefersReadinessUntilNextConnection(t *testing.T) {
	announcementError := errors.New("send failed")
	announcements := 0
	connected := 0
	var warning string
	announce := func(_ context.Context, _ types.Presence) error {
		announcements++
		if announcements == 1 {
			return announcementError
		}
		return nil
	}
	warn := func(format string, args ...any) { warning = fmt.Sprintf(format, args...) }

	if handleConnected(announce, func() { connected++ }, warn) {
		t.Fatal("failed announcement must not report readiness")
	}
	if connected != 0 {
		t.Fatalf("connected callbacks after failure = %d, want 0", connected)
	}
	if !strings.Contains(warning, "presence subscriptions will wait for the next connection") || !strings.Contains(warning, announcementError.Error()) {
		t.Fatalf("warning = %q, want retry context and error", warning)
	}

	if !handleConnected(announce, func() { connected++ }, warn) {
		t.Fatal("next successful connection must report readiness")
	}
	if announcements != 2 || connected != 1 {
		t.Fatalf("announcements = %d, connected callbacks = %d, want 2 and 1", announcements, connected)
	}
}
