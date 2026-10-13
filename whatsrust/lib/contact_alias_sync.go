package main

import (
	"context"
	"sort"
	"sync"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

// savedPNsMissingAlias selects only named address-book PN rows for a bounded
// server lookup. A missing local alias never authorizes guessing from a LID.
func savedPNsMissingAlias(ctx context.Context, contacts map[types.JID]types.ContactInfo, lids store.LIDStore, limit int) []types.JID {
	if limit <= 0 || lids == nil {
		return nil
	}
	seen := make(map[types.JID]struct{}, len(contacts))
	candidates := make([]types.JID, 0, len(contacts))
	for jid, contact := range contacts {
		pn := jid.ToNonAD()
		if pn.Server != types.DefaultUserServer || len(pn.User) < 7 || len(pn.User) > 15 {
			continue
		}
		validNumber := true
		for _, digit := range pn.User {
			if digit < '0' || digit > '9' {
				validNumber = false
				break
			}
		}
		if !validNumber {
			continue
		}
		full := plainContactName(contact.FullName)
		first := plainContactName(contact.FirstName)
		if (full == "" || phoneLikeName(full)) && (first == "" || phoneLikeName(first)) {
			continue
		}
		if _, exists := seen[pn]; !exists {
			seen[pn] = struct{}{}
			candidates = append(candidates, pn)
		}
	}
	sort.Slice(candidates, func(i, j int) bool { return candidates[i].String() < candidates[j].String() })
	selected := make([]types.JID, 0, min(limit, len(candidates)))
	for _, pn := range candidates {
		if ctx.Err() != nil || len(selected) == limit {
			break
		}
		lid, err := lids.GetLIDForPN(ctx, pn)
		if err == nil && lid.IsEmpty() {
			selected = append(selected, pn)
		}
	}
	return selected
}

const (
	aliasLookupBatchSize   = 16
	aliasLookupMaxContacts = 256
	aliasLookupTimeout     = 8 * time.Second
)

// runSavedAliasLookup is called off the Connected callback by a per-client
// worker. WhatsMeow stores returned server-confirmed mappings; the completion
// notification only follows a verified local readback for a requested PN.
func runSavedAliasLookup(
	ctx context.Context,
	contacts map[types.JID]types.ContactInfo,
	lids store.LIDStore,
	query func(context.Context, []string) ([]types.IsOnWhatsAppResponse, error),
	notify func(),
) {
	if query == nil || lids == nil {
		return
	}
	candidates := savedPNsMissingAlias(ctx, contacts, lids, aliasLookupMaxContacts)
	changed := false
	for start := 0; start < len(candidates) && ctx.Err() == nil; start += aliasLookupBatchSize {
		end := min(start+aliasLookupBatchSize, len(candidates))
		phones := make([]string, 0, end-start)
		requested := make(map[types.JID]struct{}, end-start)
		for _, pn := range candidates[start:end] {
			phones = append(phones, "+"+pn.User)
			requested[pn] = struct{}{}
		}
		batchCtx, cancel := context.WithTimeout(ctx, aliasLookupTimeout)
		responses, err := query(batchCtx, phones)
		cancel()
		if err != nil {
			break
		}
		for _, response := range responses {
			pn := response.PhoneNumber.ToNonAD()
			lid := response.JID.ToNonAD()
			if pn.Server != types.DefaultUserServer || lid.Server != types.HiddenUserServer {
				continue
			}
			if _, ok := requested[pn]; !ok {
				continue
			}
			stored, err := lids.GetLIDForPN(ctx, pn)
			if err == nil && !stored.IsEmpty() && stored.ToNonAD() == lid {
				changed = true
			}
		}
	}
	if changed && ctx.Err() != context.Canceled && notify != nil {
		notify()
	}
}

type contactAliasSyncState struct {
	mu         sync.Mutex
	cancel     context.CancelFunc
	generation uint64
}

func (state *contactAliasSyncState) start(
	load func(context.Context) (map[types.JID]types.ContactInfo, error),
	lids store.LIDStore,
	query func(context.Context, []string) ([]types.IsOnWhatsAppResponse, error),
	notify func(),
) {
	if load == nil || lids == nil || query == nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	state.mu.Lock()
	if state.cancel != nil {
		state.mu.Unlock()
		cancel()
		return
	}
	state.cancel = cancel
	state.generation++
	generation := state.generation
	state.mu.Unlock()

	go func() {
		defer cancel()
		contacts, err := load(ctx)
		if err != nil || ctx.Err() != nil {
			return
		}
		runSavedAliasLookup(ctx, contacts, lids, query, func() {
			// A completion and stop must be ordered: this callback only enqueues
			// the payload-free event, without reading the client or waiting for UI.
			state.mu.Lock()
			defer state.mu.Unlock()
			if state.generation == generation && state.cancel != nil && ctx.Err() == nil && notify != nil {
				notify()
			}
		})
	}()
}

func (state *contactAliasSyncState) stop() {
	state.mu.Lock()
	if state.cancel != nil {
		state.cancel()
		state.cancel = nil
	}
	state.generation++
	state.mu.Unlock()
}

// The lifecycle lock orders a captured client's event/start against replacement,
// logout and explicit disconnect. The caller must hold state.mu.
func (state *clientLifecycleState) allowsConnectedLocked(client *whatsmeow.Client) bool {
	return client != nil && state.client == client && !state.aliasLookupPaused && !state.resetInProgress && state.logoutDone == nil
}

func (state *clientLifecycleState) allowsConnectedFor(client *whatsmeow.Client) bool {
	state.mu.Lock()
	defer state.mu.Unlock()
	return state.allowsConnectedLocked(client)
}

func (state *clientLifecycleState) dispatchConnectedFor(client *whatsmeow.Client, connected connectedReadyFunc) bool {
	state.mu.Lock()
	defer state.mu.Unlock()
	if !state.allowsConnectedLocked(client) {
		return false
	}
	// The registered Rust callback only sends to an unbounded channel; keeping
	// the lock until it returns linearizes UI ingress with disconnect/logout.
	connected()
	return true
}

func (state *clientLifecycleState) startSavedAliasLookupFor(
	client *whatsmeow.Client,
	load func(context.Context) (map[types.JID]types.ContactInfo, error),
	lids store.LIDStore,
	query func(context.Context, []string) ([]types.IsOnWhatsAppResponse, error),
	notify func(),
) bool {
	state.mu.Lock()
	defer state.mu.Unlock()
	if !state.allowsConnectedLocked(client) {
		return false
	}
	state.aliasSync.start(load, lids, query, notify)
	return true
}

// A server websocket disconnect permits a later automatic reconnect.
func (state *clientLifecycleState) stopSavedAliasLookupFor(client *whatsmeow.Client) {
	state.mu.Lock()
	defer state.mu.Unlock()
	if client != nil && state.client == client {
		state.aliasSync.stop()
	}
}

// An explicit disconnect blocks stale Connected events until C_Connect resumes.
func (state *clientLifecycleState) pauseSavedAliasLookupFor(client *whatsmeow.Client) {
	state.mu.Lock()
	defer state.mu.Unlock()
	if client != nil && state.client == client {
		state.aliasLookupPaused = true
		state.aliasSync.stop()
	}
}

func (state *clientLifecycleState) resumeSavedAliasLookupFor(client *whatsmeow.Client) {
	state.mu.Lock()
	defer state.mu.Unlock()
	if client != nil && state.client == client && state.logoutDone == nil && !state.resetInProgress {
		state.aliasLookupPaused = false
	}
}
