package main

import (
	"context"
	"errors"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
)

type chatReadStateClient interface {
	SendAppState(context.Context, appstate.PatchInfo) error
	FetchAppState(context.Context, appstate.WAPatchName, bool, bool) error
}

type chatReadRecoveryScope func(context.Context, appstate.WAPatchName, func(context.Context) error) (bool, error)

// Internal only: errors must be safely classified before any diagnostic export.
type chatReadStageObserver func(stage string, err error)

type chatReadRecoveryLease struct{ cancel context.CancelFunc }

type chatReadRecoveryState struct {
	owner     *whatsmeow.Client
	attempted map[appstate.WAPatchName]bool
	active    map[appstate.WAPatchName]*chatReadRecoveryLease
}

func rejectedChatReadLTHash(err error) bool {
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) {
		return false
	}
	var multiple interface{ Unwrap() []error }
	if !errors.As(err, &multiple) {
		return false
	}
	causes := multiple.Unwrap()
	return len(causes) == 2 && errors.Is(causes[0], whatsmeow.ErrAppStateUpdate) &&
		causes[1] != nil && errors.Is(causes[1], appstate.ErrMismatchingLTHash)
}

// Observers run synchronously, only after an operation actually returns.
func sendChatReadStatePatch(ctx context.Context, patch appstate.PatchInfo, client chatReadStateClient, scope chatReadRecoveryScope, observers ...chatReadStageObserver) error {
	observe := func(stage string, err error) {
		for _, observer := range observers {
			if observer != nil {
				observer(stage, err)
			}
		}
	}
	err := client.SendAppState(ctx, patch)
	observe("initial_send", err)
	// The pinned concrete SendAppState returns this compound rejection before
	// its conflict retry, not after an accepted send's follow-up fetch failure.
	if !rejectedChatReadLTHash(err) || scope == nil || ctx.Err() != nil {
		return err
	}
	attempted, recoveryErr := scope(ctx, patch.Type, func(active context.Context) error {
		if err := active.Err(); err != nil {
			return err
		}
		fetchErr := client.FetchAppState(active, patch.Type, true, false)
		observe("recovery_fetch", fetchErr)
		if fetchErr != nil {
			return fetchErr
		}
		if err := active.Err(); err != nil {
			return err
		}
		resendErr := client.SendAppState(active, patch)
		observe("resend", resendErr)
		return resendErr
	})
	if !attempted {
		return err
	}
	return recoveryErr
}

func (state *clientLifecycleState) withChatReadRecovery(ctx context.Context, client *whatsmeow.Client, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
	state.mu.Lock()
	if client == nil || state.client != client || state.chatReadRecovery.owner != client ||
		state.resetInProgress || state.logoutDone != nil || state.logoutCallbackActive ||
		client.EmitAppStateEventsOnFullSync || ctx.Err() != nil || state.chatReadRecovery.attempted[name] {
		state.mu.Unlock()
		return false, nil
	}
	active, cancel := context.WithCancel(ctx)
	lease := &chatReadRecoveryLease{cancel: cancel}
	if state.chatReadRecovery.attempted == nil {
		state.chatReadRecovery.attempted = make(map[appstate.WAPatchName]bool)
	}
	if state.chatReadRecovery.active == nil {
		state.chatReadRecovery.active = make(map[appstate.WAPatchName]*chatReadRecoveryLease)
	}
	state.chatReadRecovery.attempted[name] = true
	state.chatReadRecovery.active[name] = lease
	state.mu.Unlock()
	defer func() {
		cancel()
		state.mu.Lock()
		defer state.mu.Unlock()
		if state.chatReadRecovery.active[name] == lease {
			delete(state.chatReadRecovery.active, name)
		}
	}()
	return true, operation(active)
}

// Called with the lifecycle mutex held. Never wait here: whatsmeow's app-state
// mutex acquisition is not context-cancellable, and retirement must reach
// Disconnect. A captured client's existing operations remain independently valid.
func (state *clientLifecycleState) stopChatReadRecoveryLocked() {
	for _, lease := range state.chatReadRecovery.active {
		lease.cancel()
	}
	state.chatReadRecovery.active = nil
}
