package main

import (
	"context"
	"sync/atomic"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/types"
)

func TestContactAliasSyncStartsOffCallbackOnceAndCancels(t *testing.T) {
	pn := types.NewJID("15550000001", types.DefaultUserServer)
	state := &contactAliasSyncState{}
	var queryCalls, notifications atomic.Int32
	queryStarted := make(chan struct{}, 2)
	queryDone := make(chan struct{}, 2)
	deadlineRemaining := make(chan time.Duration, 1)
	load := func(ctx context.Context) (map[types.JID]types.ContactInfo, error) {
		if deadline, ok := ctx.Deadline(); ok {
			deadlineRemaining <- time.Until(deadline)
		} else {
			deadlineRemaining <- -1
		}
		return map[types.JID]types.ContactInfo{pn: {FullName: "Alice"}}, nil
	}
	query := func(ctx context.Context, _ []string) ([]types.IsOnWhatsAppResponse, error) {
		queryCalls.Add(1)
		queryStarted <- struct{}{}
		defer func() { queryDone <- struct{}{} }()
		<-ctx.Done()
		return nil, ctx.Err()
	}
	returned := make(chan struct{})
	go func() {
		state.start(load, aliasSelectionLIDs{}, query, func() { notifications.Add(1) })
		close(returned)
	}()
	defer state.stop()
	select {
	case <-returned:
	case <-time.After(5 * time.Second):
		t.Fatal("Connected callback blocked on alias lookup")
	}
	select {
	case <-queryStarted:
	case <-time.After(5 * time.Second):
		t.Fatal("alias worker did not query the saved PN")
	}
	if remaining := <-deadlineRemaining; remaining <= 0 || remaining > 60*time.Second {
		t.Fatalf("overall lookup deadline remaining = %s, want 0 < duration <= 60s", remaining)
	}
	duplicateReturned := make(chan struct{})
	go func() {
		state.start(load, aliasSelectionLIDs{}, query, func() { notifications.Add(1) })
		close(duplicateReturned)
	}()
	select {
	case <-duplicateReturned:
	case <-time.After(5 * time.Second):
		t.Fatal("duplicate Connected callback blocked on alias lookup")
	}
	state.stop()
	select {
	case <-queryDone:
	case <-time.After(5 * time.Second):
		t.Fatal("disconnect did not cancel the in-flight alias lookup")
	}
	if got := queryCalls.Load(); got != 1 {
		t.Errorf("query calls = %d, want 1 per connection", got)
	}
	if got := notifications.Load(); got != 0 {
		t.Errorf("notifications after cancellation = %d, want 0", got)
	}
}
