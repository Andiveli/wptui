package main

/*
#include <stdint.h>
typedef const char* JID;
*/
import "C"

import (
	"context"
	"errors"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

type sendPresenceFunc func(context.Context, types.Presence) error
type connectedReadyFunc func()
type presenceWarningFunc func(string, ...any)

func handleConnected(sendPresence sendPresenceFunc, connected connectedReadyFunc, warn presenceWarningFunc) bool {
	if err := sendPresence(context.Background(), types.PresenceAvailable); err != nil {
		warn("Failed to announce presence after connection; presence subscriptions will wait for the next connection: %v", err)
		return false
	}
	connected()
	return true
}

func configurePresenceSubscriptions(client *whatsmeow.Client) {
	client.ErrorOnSubscribePresenceWithoutToken = true
}

const (
	subscribePresenceAccepted uint8 = iota
	subscribePresenceNoPrivacyToken
	subscribePresenceRejected
)

//export C_SubscribePresence
func C_SubscribePresence(cjid C.JID) C.uint8_t {
	jid := cToJid(cjid)
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil {
		return C.uint8_t(subscribePresenceRejected)
	}
	result := subscribePresence(jid, clientSnapshot.SubscribePresence)
	if result == subscribePresenceNoPrivacyToken {
		LOG_WARN("Failed to subscribe to presence: no privacy token")
	} else if result == subscribePresenceRejected {
		LOG_WARN("Failed to subscribe to presence")
	}
	return C.uint8_t(result)
}

type subscribePresenceFunc func(context.Context, types.JID) error

func subscribePresence(jid types.JID, subscribe subscribePresenceFunc) uint8 {
	if jid.Server != types.DefaultUserServer {
		return subscribePresenceRejected
	}
	if err := subscribe(context.Background(), jid); err != nil {
		if errors.Is(err, whatsmeow.ErrNoPrivacyToken) {
			return subscribePresenceNoPrivacyToken
		}
		return subscribePresenceRejected
	}
	return subscribePresenceAccepted
}
