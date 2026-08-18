package main

import (
	"context"

	"go.mau.fi/whatsmeow/types"
)

type chatSettingsLookup func(context.Context, types.JID) (types.LocalChatSettings, error)
type chatSettingsJIDMapping func(context.Context, types.JID) (types.JID, error)

type chatSettingsPayload struct {
	found      bool
	mutedUntil int64
	pinned     bool
	archived   bool
}

func lookupChatSettings(
	ctx context.Context,
	jid types.JID,
	lookup chatSettingsLookup,
	lidForPN chatSettingsJIDMapping,
	pnForLID chatSettingsJIDMapping,
) (types.LocalChatSettings, error) {
	settings, err := lookup(ctx, jid)
	if err != nil || settings.Found {
		return settings, err
	}

	var alternate types.JID
	switch jid.Server {
	case types.DefaultUserServer:
		alternate, _ = lidForPN(ctx, jid)
	case types.HiddenUserServer:
		alternate, _ = pnForLID(ctx, jid)
	}
	if alternate.IsEmpty() {
		return settings, nil
	}

	if alternateSettings, alternateErr := lookup(ctx, alternate.ToNonAD()); alternateErr == nil && alternateSettings.Found {
		return alternateSettings, nil
	}
	return settings, nil
}

func chatSettingsPayloadFrom(settings types.LocalChatSettings) chatSettingsPayload {
	mutedUntil := int64(0)
	if !settings.MutedUntil.IsZero() {
		mutedUntil = settings.MutedUntil.Unix()
	}
	return chatSettingsPayload{
		found:      settings.Found,
		mutedUntil: mutedUntil,
		pinned:     settings.Pinned,
		archived:   settings.Archived,
	}
}
