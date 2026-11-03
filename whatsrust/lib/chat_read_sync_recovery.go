package main

import (
	"context"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
)

type chatReadStateClient interface {
	SendAppState(context.Context, appstate.PatchInfo) error
	FetchAppState(context.Context, appstate.WAPatchName, bool, bool) error
}

type chatReadRecoveryScope func(context.Context, appstate.WAPatchName, func(context.Context) error) (bool, error)

// This baseline seam preserves the direct send while isolated recovery tests
// establish the behavioral RED before any recovery is enabled.
func sendChatReadStatePatch(ctx context.Context, patch appstate.PatchInfo, client chatReadStateClient, scope chatReadRecoveryScope) error {
	return client.SendAppState(ctx, patch)
}

func (state *clientLifecycleState) withChatReadRecovery(ctx context.Context, client *whatsmeow.Client, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
	return true, operation(ctx)
}

// Called with the lifecycle mutex held. The baseline adds no lifecycle work.
func (state *clientLifecycleState) stopChatReadRecoveryLocked() {}
