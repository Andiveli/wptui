package main

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestNormalizePresenceJID(t *testing.T) {
	lid := types.NewJID("alice", types.HiddenUserServer)
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	lookupError := errors.New("lookup failed")

	tests := []struct {
		name    string
		from    types.JID
		mapped  types.JID
		err     error
		want    types.JID
		lookups int
	}{
		{name: "LID maps to canonical PN", from: lid, mapped: pn, want: pn, lookups: 1},
		{name: "missing mapping retains LID", from: lid, want: lid, lookups: 1},
		{name: "lookup failure retains LID", from: lid, err: lookupError, want: lid, lookups: 1},
		{name: "PN bypasses mapping", from: pn, want: pn},
		{name: "group bypasses mapping", from: types.NewJID("group", types.GroupServer), want: types.NewJID("group", types.GroupServer)},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			lookups := 0
			got, _ := normalizePresenceJID(context.Background(), tt.from, func(_ context.Context, gotLID types.JID) (types.JID, error) {
				lookups++
				if gotLID != lid {
					t.Fatalf("lookup JID = %s, want %s", gotLID, lid)
				}
				return tt.mapped, tt.err
			})
			if got != tt.want || lookups != tt.lookups {
				t.Fatalf("normalized JID = %s, lookups = %d; want %s, %d", got, lookups, tt.want, tt.lookups)
			}
		})
	}
}

func TestDispatchPresenceEventMapsLIDAndPreservesCallbackPayload(t *testing.T) {
	lid := types.NewJID("private-lid", types.HiddenUserServer)
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	event := &events.Presence{From: lid, Unavailable: true, LastSeen: time.Unix(42, 0)}
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	dispatches := 0

	dispatchPresenceEvent(event, func(ctx context.Context, got types.JID) (types.JID, error) {
		if _, ok := ctx.Deadline(); !ok {
			t.Fatal("LID lookup context has no deadline")
		}
		if got != lid {
			t.Fatalf("lookup JID = %s, want %s", got, lid)
		}
		return pn, nil
	}, diagnostics.record, diagnostics.update, func(from string, unavailable bool, lastSeen int64) {
		dispatches++
		if from != pn.String() || !unavailable || lastSeen != 42 {
			t.Fatalf("callback parameters = (%q, %t, %d), want (%q, true, 42)", from, unavailable, lastSeen, pn.String())
		}
	})

	if dispatches != 1 {
		t.Fatalf("dispatches = %d, want 1", dispatches)
	}
	if report := diagnostics.drain(); !strings.Contains(report, "normalized=pn, normalization=ok, dispatch=called") {
		t.Fatalf("pipeline outcome missing from diagnostics:\n%s", report)
	}
}

func TestDispatchPresenceEventFallsBackAfterNormalizationTimeout(t *testing.T) {
	lid := types.NewJID("private-lid", types.HiddenUserServer)
	event := &events.Presence{From: lid}
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	dispatches := 0

	dispatchPresenceEvent(event, func(ctx context.Context, _ types.JID) (types.JID, error) {
		<-ctx.Done()
		return types.EmptyJID, ctx.Err()
	}, diagnostics.record, diagnostics.update, func(from string, unavailable bool, lastSeen int64) {
		dispatches++
		if from != lid.String() || unavailable || lastSeen != 0 {
			t.Fatalf("fallback callback parameters = (%q, %t, %d)", from, unavailable, lastSeen)
		}
	})

	if dispatches != 1 {
		t.Fatalf("dispatches = %d, want 1", dispatches)
	}
	if report := diagnostics.drain(); !strings.Contains(report, "normalized=fallback-lid, normalization=timeout, dispatch=called") {
		t.Fatalf("timeout outcome missing from diagnostics:\n%s", report)
	}
}
