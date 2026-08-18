package main

import (
	"context"
	"errors"
	"time"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

const presenceNormalizationTimeout = 250 * time.Millisecond

type lidToPNFunc func(context.Context, types.JID) (types.JID, error)

func normalizePresenceJID(ctx context.Context, jid types.JID, getPNForLID lidToPNFunc) (types.JID, string) {
	if jid.Server != types.HiddenUserServer {
		return jid, "not-needed"
	}
	pn, err := getPNForLID(ctx, jid)
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) || errors.Is(ctx.Err(), context.DeadlineExceeded) {
			return jid, "timeout"
		}
		return jid, "error"
	}
	if pn.IsEmpty() {
		return jid, "missing"
	}
	return pn, "ok"
}

func dispatchPresenceEvent(event *events.Presence, getPNForLID lidToPNFunc, record func(*events.Presence) uint64, update func(uint64, string, string, string), dispatch func(string, bool, int64)) {
	sequence := record(event)
	ctx, cancel := context.WithTimeout(context.Background(), presenceNormalizationTimeout)
	defer cancel()
	from, normalization := normalizePresenceJID(ctx, event.From, getPNForLID)
	normalized := "fallback-lid"
	if from.Server == types.DefaultUserServer {
		normalized = "pn"
	}
	lastSeen := int64(0)
	if !event.LastSeen.IsZero() {
		lastSeen = event.LastSeen.Unix()
	}
	dispatch(from.ToNonAD().String(), event.Unavailable, lastSeen)
	update(sequence, normalized, normalization, "called")
}
