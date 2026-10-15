package main

import "go.mau.fi/whatsmeow"

// Check before presence and again under the lifecycle lock immediately before
// the UI event: disconnect may happen while SendPresence is in flight.
func dispatchConnectedIfCurrent(
	state *clientLifecycleState,
	client *whatsmeow.Client,
	sendPresence sendPresenceFunc,
	connected connectedReadyFunc,
	warn presenceWarningFunc,
) bool {
	if !state.allowsConnectedFor(client) {
		return false
	}
	var delivered bool
	if !handleConnected(sendPresence, func() {
		delivered = state.dispatchConnectedFor(client, connected)
	}, warn) {
		return false
	}
	return delivered
}

func dispatchConnectionFamily(client *whatsmeow.Client) {
	if !dispatchConnectedIfCurrent(&lifecycleState, client, client.SendPresence, dispatchConnectedEvent, LOG_WARN) {
		return
	}
	if client.Store == nil || client.Store.Contacts == nil || client.Store.LIDs == nil {
		return
	}
	lifecycleState.startSavedAliasLookupFor(
		client,
		client.Store.Contacts.GetAllContacts,
		client.Store.LIDs,
		client.IsOnWhatsApp,
		dispatchContactAliasesUpdatedEvent,
	)
}
