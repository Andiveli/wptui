package main

import (
	"context"
	"sync/atomic"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

func TestAliasLifecycleCancelsRetiredClientWithoutStoppingReplacement(t *testing.T) {
	pn := types.NewJID("15550000001", types.DefaultUserServer)
	load := func(context.Context) (map[types.JID]types.ContactInfo, error) {
		return map[types.JID]types.ContactInfo{pn: {FullName: "Alice"}}, nil
	}
	first := &whatsmeow.Client{}
	second := &whatsmeow.Client{}
	state := &clientLifecycleState{}
	state.publishClient(first)
	defer state.publishClient(nil)
	var notifications atomic.Int32
	firstStarted := make(chan context.Context, 1)
	firstDone := make(chan struct{}, 1)
	firstQuery := func(ctx context.Context, _ []string) ([]types.IsOnWhatsAppResponse, error) {
		firstStarted <- ctx
		<-ctx.Done()
		firstDone <- struct{}{}
		return nil, ctx.Err()
	}
	if !state.startSavedAliasLookupFor(first, load, aliasSelectionLIDs{}, firstQuery, func() { notifications.Add(1) }) {
		t.Fatal("current first client did not reserve an alias worker")
	}
	awaitAliasSignal(t, firstStarted, "first client lookup")
	state.publishClient(second)
	awaitAliasSignal(t, firstDone, "retired client cancellation")

	secondStarted := make(chan context.Context, 2)
	secondDone := make(chan struct{}, 2)
	secondQuery := func(ctx context.Context, _ []string) ([]types.IsOnWhatsAppResponse, error) {
		secondStarted <- ctx
		<-ctx.Done()
		secondDone <- struct{}{}
		return nil, ctx.Err()
	}
	if !state.startSavedAliasLookupFor(second, load, aliasSelectionLIDs{}, secondQuery, func() { notifications.Add(1) }) {
		t.Fatal("replacement client did not reserve an alias worker")
	}
	secondCtx := awaitAliasSignal(t, secondStarted, "replacement client lookup")
	state.stopSavedAliasLookupFor(first) // A retired callback can arrive after replacement.
	if secondCtx.Err() != nil {
		t.Fatalf("late old-client disconnect canceled replacement: %v", secondCtx.Err())
	}
	state.stopSavedAliasLookupFor(second) // Automatic websocket disconnect.
	awaitAliasSignal(t, secondDone, "automatic disconnect cancellation")
	if !state.startSavedAliasLookupFor(second, load, aliasSelectionLIDs{}, secondQuery, nil) {
		t.Fatal("automatic reconnect did not rearm alias lookup")
	}
	awaitAliasSignal(t, secondStarted, "automatic reconnect lookup")
	state.pauseSavedAliasLookupFor(second) // Explicit disconnect or logout.
	awaitAliasSignal(t, secondDone, "replacement client cancellation")
	if state.startSavedAliasLookupFor(second, load, aliasSelectionLIDs{}, secondQuery, nil) {
		t.Fatal("stale Connected started a worker after explicit disconnect")
	}
	state.resumeSavedAliasLookupFor(second) // Explicit reconnect.
	if !state.startSavedAliasLookupFor(second, load, aliasSelectionLIDs{}, secondQuery, nil) {
		t.Fatal("explicit reconnect did not rearm alias lookup")
	}
	awaitAliasSignal(t, secondStarted, "reconnected client lookup")
	state.pauseSavedAliasLookupFor(second)
	awaitAliasSignal(t, secondDone, "reconnected client cancellation")
	if got := notifications.Load(); got != 0 {
		t.Fatalf("notifications after cancellation = %d, want 0", got)
	}
}

func awaitAliasSignal[T any](t *testing.T, ch <-chan T, name string) T {
	t.Helper()
	select {
	case value := <-ch:
		return value
	case <-time.After(5 * time.Second):
		t.Fatalf("timed out waiting for %s", name)
		var zero T
		return zero
	}
}
