package main

import (
	"context"
	"errors"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/types"
)

func TestLookupChatSettingsUsesEquivalentJIDWhenPrimaryIsMissing(t *testing.T) {
	ctx := context.Background()
	lid := types.NewJID("alice", types.HiddenUserServer)
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	want := types.LocalChatSettings{Found: true, Pinned: true}

	got, err := lookupChatSettings(ctx, pn, func(_ context.Context, jid types.JID) (types.LocalChatSettings, error) {
		if jid == pn {
			return types.LocalChatSettings{}, nil
		}
		if jid == lid {
			return want, nil
		}
		t.Fatalf("unexpected lookup JID: %s", jid)
		return types.LocalChatSettings{}, nil
	}, func(_ context.Context, jid types.JID) (types.JID, error) {
		if jid != pn {
			t.Fatalf("mapping JID = %s, want %s", jid, pn)
		}
		return lid, nil
	}, nil)

	if err != nil || got != want {
		t.Fatalf("settings = %#v, err = %v; want %#v, nil", got, err, want)
	}
}

func TestLookupChatSettingsPreservesPrimaryError(t *testing.T) {
	wantErr := errors.New("settings unavailable")
	got, err := lookupChatSettings(context.Background(), types.NewJID("1", types.DefaultUserServer), func(context.Context, types.JID) (types.LocalChatSettings, error) {
		return types.LocalChatSettings{}, wantErr
	}, nil, nil)

	if !errors.Is(err, wantErr) || got.Found {
		t.Fatalf("settings = %#v, err = %v; want empty settings and original error", got, err)
	}
}

func TestChatSettingsPayloadFromUsesUnixSecondsAndZeroForUnsetTime(t *testing.T) {
	settings := types.LocalChatSettings{
		Found:      true,
		MutedUntil: time.Unix(123, 456).In(time.FixedZone("offset", 3600)),
		Pinned:     true,
		Archived:   true,
	}

	got := chatSettingsPayloadFrom(settings)
	want := chatSettingsPayload{found: true, mutedUntil: 123, pinned: true, archived: true}
	if got != want {
		t.Fatalf("payload = %#v, want %#v", got, want)
	}
	if zero := chatSettingsPayloadFrom(types.LocalChatSettings{}); zero.mutedUntil != 0 {
		t.Fatalf("unset muted time = %d, want 0", zero.mutedUntil)
	}
}
