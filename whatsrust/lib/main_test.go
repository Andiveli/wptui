package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestJidsMatchSelfNormalizesDeviceIdentity(t *testing.T) {
	self := types.NewADJID("self", 0, 7)
	candidate := types.NewJID("self", types.DefaultUserServer)
	if !jidsMatchSelf(self, candidate) {
		t.Fatal("device and user JIDs should identify the same participant")
	}
	if jidsMatchSelf(self, types.NewJID("other", types.DefaultUserServer)) {
		t.Fatal("different participants must not match")
	}
}

func TestMarkReadResultPropagatesFailureWithoutNetwork(t *testing.T) {
	if got := markReadResult(nil); got != 1 {
		t.Fatal("nil sender must fail")
	}
	if got := markReadResult(func() error { return errors.New("transient") }); got != 2 {
		t.Fatal("sender failure must propagate")
	}
	if got := markReadResult(func() error { return nil }); got != 0 {
		t.Fatal("successful sender must return success")
	}
}

func TestPinnedPresenceContractAndNarrowSubscription(t *testing.T) {
	lastSeen := time.Unix(42, 0)
	event := &events.Presence{
		From:        types.NewJID("12345", types.DefaultUserServer),
		Unavailable: true,
		LastSeen:    lastSeen,
	}
	if event.From.User != "12345" || !event.Unavailable || event.LastSeen.Unix() != 42 {
		t.Fatalf("unexpected pinned presence contract: %#v", event)
	}

	calls := 0
	subscribe := func(_ context.Context, jid types.JID) error {
		calls++
		if jid != event.From {
			t.Fatalf("subscribed to %s, want %s", jid, event.From)
		}
		return nil
	}
	if got := subscribePresence(event.From, subscribe); got != subscribePresenceAccepted || calls != 1 {
		t.Fatalf("individual subscription result=%d calls=%d", got, calls)
	}
	if got := subscribePresence(types.NewJID("group", types.GroupServer), subscribe); got != subscribePresenceRejected || calls != 1 {
		t.Fatalf("group subscription must be rejected before whatsmeow, calls=%d", calls)
	}
}

func TestCommunityEntryIncludedDistinguishesRootsAndLinkedGroups(t *testing.T) {
	tests := []struct {
		name        string
		isParent    bool
		parentEmpty bool
		want        bool
	}{
		{name: "community root", isParent: true, parentEmpty: true, want: true},
		{name: "linked group", parentEmpty: false, want: true},
		{name: "ordinary group", parentEmpty: true, want: false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := communityEntryIncluded(tt.isParent, tt.parentEmpty); got != tt.want {
				t.Fatalf("communityEntryIncluded(%t, %t) = %t, want %t", tt.isParent, tt.parentEmpty, got, tt.want)
			}
		})
	}
}

func TestFreeCommunitiesNilResultIsSafe(t *testing.T) {
	freeCommunityEntries(nil)
}

func TestRawPresenceDiagnosticsDisabledIsNoOp(t *testing.T) {
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(false)
	diagnostics.record(&events.Presence{From: types.NewJID("private-user", types.DefaultUserServer)})
	if report := diagnostics.drain(); report != "" {
		t.Fatalf("disabled diagnostics returned %q", report)
	}
}

func TestRawPresenceDiagnosticsAreBoundedOrderedAndRedacted(t *testing.T) {
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	for index := 0; index <= maxRawPresenceDiagnosticEntries; index++ {
		diagnostics.record(&events.Presence{
			From:        types.NewJID(fmt.Sprintf("private-user-%d", index), types.DefaultUserServer),
			Unavailable: index%2 == 0,
			LastSeen:    time.Unix(int64(index+1), 0),
		})
	}

	report := diagnostics.drain()
	if strings.Contains(report, "private-user") {
		t.Fatal("raw presence report exposed JID user data")
	}
	if !strings.Contains(report, "raw presence events received: 51\n") || strings.Contains(report, "\n1. server=") {
		t.Fatalf("unexpected bounded report header/order:\n%s", report)
	}
	if !strings.Contains(report, "2. server=s.whatsapp.net, unavailable=false, last_seen_present=true") ||
		!strings.Contains(report, "51. server=s.whatsapp.net, unavailable=true, last_seen_present=true") {
		t.Fatalf("unexpected retained event order:\n%s", report)
	}
}

func TestRawPresenceDiagnosticsClassifyServersAndResetOnDrain(t *testing.T) {
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	for _, event := range []*events.Presence{
		{From: types.NewJID("secret-a", types.DefaultUserServer)},
		{From: types.NewJID("secret-b", types.HiddenUserServer), Unavailable: true},
		{From: types.NewJID("secret-c", types.GroupServer)},
	} {
		diagnostics.record(event)
	}

	report := diagnostics.drain()
	for _, expected := range []string{"server=s.whatsapp.net", "server=lid", "server=other"} {
		if !strings.Contains(report, expected) {
			t.Fatalf("report missing %q:\n%s", expected, report)
		}
	}
	if strings.Contains(report, "secret-") || !strings.Contains(report, "last_seen_present=false") {
		t.Fatalf("report leaked identity or omitted safe metadata:\n%s", report)
	}
	if report := diagnostics.drain(); report != "raw presence events received: 0\n" {
		t.Fatalf("drain did not reset run data: %q", report)
	}
	diagnostics.reset(true)
	if report := diagnostics.drain(); report != "raw presence events received: 0\n" {
		t.Fatalf("run reset retained data: %q", report)
	}
}

func TestSubscribePresenceResultIdentifiesNoPrivacyToken(t *testing.T) {
	jid := types.NewJID("12345", types.DefaultUserServer)
	tests := []struct {
		name string
		err  error
		want uint8
	}{
		{name: "valid token accepted", want: subscribePresenceAccepted},
		{name: "missing token identifiable", err: fmt.Errorf("wrapped: %w", whatsmeow.ErrNoPrivacyToken), want: subscribePresenceNoPrivacyToken},
		{name: "transport error rejected", err: errors.New("transport failed"), want: subscribePresenceRejected},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := subscribePresence(jid, func(context.Context, types.JID) error { return tt.err })
			if got != tt.want {
				t.Fatalf("subscribePresence() = %d, want %d", got, tt.want)
			}
		})
	}
}

func TestPresenceSubscriptionsRequirePrivacyToken(t *testing.T) {
	client := &whatsmeow.Client{}
	configurePresenceSubscriptions(client)
	if !client.ErrorOnSubscribePresenceWithoutToken {
		t.Fatal("presence subscriptions must reject missing privacy tokens")
	}
}

func TestNormalizePresenceJID(t *testing.T) {
	lid := types.NewJID("alice", types.HiddenUserServer)
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	lookupError := errors.New("lookup failed")

	tests := []struct {
		name    string
		from    types.JID
		mapped  types.JID
		err     error
		want    types.JID
		lookups int
	}{
		{name: "LID maps to canonical PN", from: lid, mapped: pn, want: pn, lookups: 1},
		{name: "missing mapping retains LID", from: lid, want: lid, lookups: 1},
		{name: "lookup failure retains LID", from: lid, err: lookupError, want: lid, lookups: 1},
		{name: "PN bypasses mapping", from: pn, want: pn},
		{name: "group bypasses mapping", from: types.NewJID("group", types.GroupServer), want: types.NewJID("group", types.GroupServer)},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			lookups := 0
			got, _ := normalizePresenceJID(context.Background(), tt.from, func(_ context.Context, gotLID types.JID) (types.JID, error) {
				lookups++
				if gotLID != lid {
					t.Fatalf("lookup JID = %s, want %s", gotLID, lid)
				}
				return tt.mapped, tt.err
			})
			if got != tt.want || lookups != tt.lookups {
				t.Fatalf("normalized JID = %s, lookups = %d; want %s, %d", got, lookups, tt.want, tt.lookups)
			}
		})
	}
}

func TestDispatchPresenceEventMapsLIDAndDispatchesCompleteCPayloadOnce(t *testing.T) {
	lid := types.NewJID("private-lid", types.HiddenUserServer)
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	event := &events.Presence{From: lid, Unavailable: true, LastSeen: time.Unix(42, 0)}
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	dispatches := 0

	dispatchPresenceEvent(event, func(ctx context.Context, got types.JID) (types.JID, error) {
		if _, ok := ctx.Deadline(); !ok {
			t.Fatal("LID lookup context has no deadline")
		}
		if got != lid {
			t.Fatalf("lookup JID = %s, want %s", got, lid)
		}
		return pn, nil
	}, diagnostics.record, diagnostics.update, func(from string, unavailable bool, lastSeen int64) {
		dispatches++
		if from != pn.String() || !unavailable || lastSeen != 42 {
			t.Fatalf("callback parameters = (%q, %t, %d), want (%q, true, 42)", from, unavailable, lastSeen, pn.String())
		}
	})

	if dispatches != 1 {
		t.Fatalf("dispatches = %d, want 1", dispatches)
	}
	report := diagnostics.drain()
	if !strings.Contains(report, "normalized=pn, normalization=ok, dispatch=called") {
		t.Fatalf("pipeline outcome missing from diagnostics:\n%s", report)
	}
}

func TestDispatchPresenceEventFallsBackAndDispatchesAfterNormalizationTimeout(t *testing.T) {
	lid := types.NewJID("private-lid", types.HiddenUserServer)
	event := &events.Presence{From: lid}
	var diagnostics rawPresenceDiagnostics
	diagnostics.reset(true)
	dispatches := 0

	dispatchPresenceEvent(event, func(ctx context.Context, _ types.JID) (types.JID, error) {
		<-ctx.Done()
		return types.EmptyJID, ctx.Err()
	}, diagnostics.record, diagnostics.update, func(from string, unavailable bool, lastSeen int64) {
		dispatches++
		if from != lid.String() || unavailable || lastSeen != 0 {
			t.Fatalf("fallback callback parameters = (%q, %t, %d)", from, unavailable, lastSeen)
		}
	})

	if dispatches != 1 {
		t.Fatalf("dispatches = %d, want 1", dispatches)
	}
	report := diagnostics.drain()
	if !strings.Contains(report, "normalized=fallback-lid, normalization=timeout, dispatch=called") {
		t.Fatalf("timeout outcome missing from diagnostics:\n%s", report)
	}
}

func TestConnectedAnnouncesPresenceBeforeReportingReady(t *testing.T) {
	var calls []string

	ready := handleConnected(
		func(_ context.Context, presence types.Presence) error {
			if presence != types.PresenceAvailable {
				t.Fatalf("presence = %q, want available", presence)
			}
			calls = append(calls, "announce")
			return nil
		},
		func() { calls = append(calls, "connected") },
		func(string, ...any) { t.Fatal("unexpected warning") },
	)

	if !ready {
		t.Fatal("successful announcement must report readiness")
	}
	if got, want := strings.Join(calls, ","), "announce,connected"; got != want {
		t.Fatalf("call order = %q, want %q", got, want)
	}
}

func TestConnectedRepeatsPresenceAnnouncementAfterReconnect(t *testing.T) {
	var calls []string
	announce := func(_ context.Context, _ types.Presence) error {
		calls = append(calls, "announce")
		return nil
	}
	connected := func() { calls = append(calls, "connected") }
	warn := func(string, ...any) { t.Fatal("unexpected warning") }

	handleConnected(announce, connected, warn)
	handleConnected(announce, connected, warn)

	if got, want := strings.Join(calls, ","), "announce,connected,announce,connected"; got != want {
		t.Fatalf("reconnect call order = %q, want %q", got, want)
	}
}

func TestConnectedAnnouncementFailureDefersReadinessUntilNextConnection(t *testing.T) {
	announcementError := errors.New("send failed")
	announcements := 0
	connected := 0
	var warning string
	announce := func(_ context.Context, _ types.Presence) error {
		announcements++
		if announcements == 1 {
			return announcementError
		}
		return nil
	}
	warn := func(format string, args ...any) { warning = fmt.Sprintf(format, args...) }

	if handleConnected(announce, func() { connected++ }, warn) {
		t.Fatal("failed announcement must not report readiness")
	}
	if connected != 0 {
		t.Fatalf("connected callbacks after failure = %d, want 0", connected)
	}
	if !strings.Contains(warning, "presence subscriptions will wait for the next connection") ||
		!strings.Contains(warning, announcementError.Error()) {
		t.Fatalf("warning = %q, want retry context and error", warning)
	}

	if !handleConnected(announce, func() { connected++ }, warn) {
		t.Fatal("next successful connection must report readiness")
	}
	if announcements != 2 || connected != 1 {
		t.Fatalf("announcements = %d, connected callbacks = %d, want 2 and 1", announcements, connected)
	}
}

func profilePicturePNG(t *testing.T) []byte {
	t.Helper()
	var data bytes.Buffer
	img := image.NewRGBA(image.Rect(0, 0, 2, 2))
	img.Set(0, 0, color.White)
	if err := png.Encode(&data, img); err != nil {
		t.Fatal(err)
	}
	return data.Bytes()
}

func wrappedEditFixture(target, replacement string) *waE2E.Message {
	return &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
		ProtocolMessage: &waE2E.ProtocolMessage{
			Key:           &waCommon.MessageKey{ID: stringPointer(target)},
			Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
			EditedMessage: &waE2E.Message{Conversation: stringPointer(replacement)},
		},
	}}}
}

func TestFetchProfilePictureAvailablePreview(t *testing.T) {
	want := profilePicturePNG(t)
	lookupCalled := false
	downloadCalled := false
	outcome := fetchProfilePicture(context.Background(), "12345@g.us", func(_ context.Context, jid types.JID, params *whatsmeow.GetProfilePictureParams) (*types.ProfilePictureInfo, error) {
		lookupCalled = true
		if jid.Server != types.GroupServer || params == nil || !params.Preview {
			t.Fatalf("unexpected lookup request: jid=%s params=%#v", jid, params)
		}
		return &types.ProfilePictureInfo{URL: "https://temporary.invalid/avatar", ID: "picture-42", Type: "preview"}, nil
	}, func(_ context.Context, url string, limit int64) ([]byte, error) {
		downloadCalled = true
		if url != "https://temporary.invalid/avatar" || limit != profilePictureMaxSize {
			t.Fatalf("unexpected download request: url=%q limit=%d", url, limit)
		}
		return want, nil
	})

	if !lookupCalled || !downloadCalled || outcome.status != profilePictureStatusAvailable || outcome.pictureID != "picture-42" || outcome.pictureType != "preview" || !bytes.Equal(outcome.data, want) {
		t.Fatalf("unexpected outcome: %#v", outcome)
	}
}

func TestPinnedWhatsmeowMessageActionBuilders(t *testing.T) {
	var _ func(*whatsmeow.Client, types.JID, types.JID, types.MessageID, string) *waE2E.Message = (*whatsmeow.Client).BuildReaction
	var _ func(*whatsmeow.Client, types.JID, types.MessageID, *waE2E.Message) *waE2E.Message = (*whatsmeow.Client).BuildEdit
	var _ func(*whatsmeow.Client, types.JID, types.JID, types.MessageID) *waE2E.Message = (*whatsmeow.Client).BuildRevoke

	client := &whatsmeow.Client{}
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	messageID := types.MessageID("audit-message")

	t.Run("reaction", func(t *testing.T) {
		message := client.BuildReaction(chat, sender, messageID, "👍")
		if message.GetReactionMessage().GetText() != "👍" || message.GetReactionMessage().GetKey().GetID() != messageID {
			t.Fatal("reaction builder did not preserve target and text")
		}
	})
	t.Run("edit", func(t *testing.T) {
		replacement := &waE2E.Message{Conversation: stringPointer("replacement")}
		message := client.BuildEdit(chat, messageID, replacement)
		protocol := message.GetEditedMessage().GetMessage().GetProtocolMessage()
		if protocol.GetType() != waE2E.ProtocolMessage_MESSAGE_EDIT || protocol.GetKey().GetID() != messageID || protocol.GetEditedMessage().GetConversation() != "replacement" {
			t.Fatal("edit builder did not preserve protocol target and content")
		}
	})
	t.Run("revoke", func(t *testing.T) {
		message := client.BuildRevoke(chat, sender, messageID)
		if message.GetProtocolMessage().GetType() != waE2E.ProtocolMessage_REVOKE || message.GetProtocolMessage().GetKey().GetID() != messageID {
			t.Fatal("revoke builder did not preserve protocol target")
		}
	})
}

func TestMessageActionEventFromIncomingMessage(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	info := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, Sender: sender},
		ID:            "edit-action",
		Timestamp:     time.Unix(42, 0),
	}
	editProtocol := func(replacement *waE2E.Message) *waE2E.ProtocolMessage {
		return &waE2E.ProtocolMessage{
			Key:           &waCommon.MessageKey{ID: stringPointer("target-message")},
			Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
			EditedMessage: replacement,
		}
	}
	wrappedEdit := func(replacement *waE2E.Message) *waE2E.Message {
		return &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
			ProtocolMessage: editProtocol(replacement),
		}}}
	}

	cases := []struct {
		name        string
		event       *events.Message
		wantBody    string
		wantAction  string
		wantTarget  string
		wantSuccess bool
	}{
		{
			name:        "live edit keeps envelope action identity and protocol target",
			event:       &events.Message{Info: info, RawMessage: wrappedEdit(&waE2E.Message{Conversation: stringPointer("conversation replacement")}), IsEdit: true},
			wantBody:    "conversation replacement",
			wantAction:  "edit-action",
			wantTarget:  "target-message",
			wantSuccess: true,
		},
		{
			name: "raw ephemeral edited-message wrapper with extended text",
			event: &events.Message{Info: info, RawMessage: &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{
				Message: wrappedEdit(&waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("extended replacement")}}),
			}}},
			wantBody:    "extended replacement",
			wantAction:  "edit-action",
			wantTarget:  "target-message",
			wantSuccess: true,
		},
		{
			name: "parsed history keeps source action identity and rewritten target",
			event: &events.Message{
				Info:         types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: sender}, ID: "target-message", Timestamp: time.Unix(43, 0)},
				RawMessage:   wrappedEdit(&waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("normalized replacement")}}),
				Message:      &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("normalized replacement")}},
				SourceWebMsg: &waWeb.WebMessageInfo{Key: &waCommon.MessageKey{ID: stringPointer("history-action")}},
			},
			wantBody:    "normalized replacement",
			wantAction:  "history-action",
			wantTarget:  "target-message",
			wantSuccess: true,
		},
		{
			name: "live normalized edit without raw envelope is ignored",
			event: &events.Message{
				Info:    info,
				Message: &waE2E.Message{Conversation: stringPointer("replacement")},
				IsEdit:  true,
			},
			wantSuccess: false,
		},
		{
			name:        "missing target key",
			event:       &events.Message{Info: info, RawMessage: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(), EditedMessage: &waE2E.Message{Conversation: stringPointer("replacement")}}}},
			wantSuccess: false,
		},
		{
			name:        "normalized edit without replacement",
			event:       &events.Message{Info: info, Message: &waE2E.Message{}, IsEdit: true},
			wantSuccess: false,
		},
		{
			name:        "ordinary text is not an edit",
			event:       &events.Message{Info: info, Message: &waE2E.Message{Conversation: stringPointer("ordinary")}},
			wantSuccess: false,
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			action, ok := messageActionEventFromIncomingMessage(testCase.event)
			if ok != testCase.wantSuccess {
				t.Fatalf("recognized = %v, want %v", ok, testCase.wantSuccess)
			}
			if !ok {
				return
			}
			if action.actionID != testCase.wantAction || action.targetMessageID != testCase.wantTarget || action.replacement != testCase.wantBody || action.chat != chat.String() || action.sender != sender.String() || action.occurredAt != testCase.event.Info.Timestamp.Unix() || action.kind != messageActionEdit {
				t.Fatalf("unexpected action: %#v", action)
			}
		})
	}
}

func TestOrdinaryActionBuildersRejectInvalidTargets(t *testing.T) {
	client := &whatsmeow.Client{}
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	newsletter := types.NewJID("channel", types.NewsletterServer)

	if _, err := buildOrdinaryReaction(client, newsletter, sender, "message", "👍"); err == nil {
		t.Fatal("newsletter reaction was accepted as ordinary")
	}
	if _, err := buildOrdinaryReaction(client, chat, types.JID{}, "message", "👍"); err == nil {
		t.Fatal("reaction without sender was accepted")
	}
	if _, err := buildOrdinaryEdit(client, newsletter, "message", "replacement"); err == nil {
		t.Fatal("newsletter edit was accepted as ordinary")
	}
	if _, err := buildOrdinaryEdit(client, chat, "message", " \t"); err == nil {
		t.Fatal("blank edit was accepted")
	}
	if _, err := buildOrdinaryRevoke(client, newsletter, sender, "message"); err == nil {
		t.Fatal("newsletter revoke was accepted as ordinary")
	}
	if _, err := buildOrdinaryRevoke(client, chat, sender, ""); err == nil {
		t.Fatal("revoke without message ID was accepted")
	}
}

func TestReactionEventExtractsReactorAndSupportsRemoval(t *testing.T) {
	group := types.NewJID("12345", types.GroupServer)
	reactor := types.NewJID("reactor", types.DefaultUserServer)
	message := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
		Key:  &waCommon.MessageKey{ID: stringPointer("target"), Participant: stringPointer("target-author@s.whatsapp.net")},
		Text: stringPointer("👍"),
	}}

	for _, testCase := range []struct {
		name string
		text string
	}{
		{name: "adds or replaces a reaction", text: "👍"},
		{name: "empty text removes a reaction", text: ""},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			message.ReactionMessage.Text = stringPointer(testCase.text)
			event, ok := reactionEventFromMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: group, Sender: reactor}}, message)
			if !ok {
				t.Fatal("expected reaction event")
			}
			if event.chat != group.String() || event.targetMessageID != "target" || event.participant != reactor.String() || event.text != testCase.text || event.isFromMe {
				t.Fatalf("unexpected reaction event: %#v", event)
			}
		})
	}
}

func TestStatusProtocolReactionDiagnosticIncludesOnlyProtocolMetadata(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:     types.NewJID("mobile", types.HiddenUserServer),
		Sender:   types.NewJID("mobile", types.HiddenUserServer),
		IsFromMe: true,
	}}
	reactionFromMe := true
	message := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
		Key: &waCommon.MessageKey{
			RemoteJID:   stringPointer("status@broadcast"),
			Participant: stringPointer("author@s.whatsapp.net"),
			FromMe:      &reactionFromMe,
			ID:          stringPointer("status-id"),
		},
		Text: stringPointer("❤️"),
	}}

	line, ok := statusProtocolReactionDiagnostic(info, message)
	if !ok {
		t.Fatal("expected reaction diagnostic")
	}
	for _, expected := range []string{
		"status_protocol=reaction", "chat=mobile@lid", "sender=mobile@lid", "from_me=true",
		"remote_jid=status@broadcast", "participant=author@s.whatsapp.net", "key_from_me=true", "id=status-id",
		"emoji_codepoints=U+2764,U+FE0F", "emoji=❤️",
	} {
		if !strings.Contains(line, expected) {
			t.Fatalf("diagnostic missing %q: %s", expected, line)
		}
	}
}

func TestStatusProtocolDiagnosticEmissionRequiresDebug(t *testing.T) {
	var emitted []string
	emit := func(format string, args ...any) { emitted = append(emitted, fmt.Sprintf(format, args...)) }

	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "")
	emitStatusProtocolDiagnostic(emit, "status_protocol=reaction")
	if len(emitted) != 0 {
		t.Fatalf("disabled diagnostic emitted %#v", emitted)
	}

	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "1")
	emitStatusProtocolDiagnostic(emit, "status_protocol=reaction")
	if got, want := emitted, []string{"status_protocol=reaction"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("emitted = %#v, want %#v", got, want)
	}
}

func TestStatusProtocolReactionDiagnosticIgnoresOrdinaryReactions(t *testing.T) {
	message := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
		Key: &waCommon.MessageKey{
			RemoteJID:   stringPointer("chat@s.whatsapp.net"),
			Participant: stringPointer("author@s.whatsapp.net"),
			ID:          stringPointer("message-id"),
		},
		Text: stringPointer("👍"),
	}}
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:   types.NewJID("chat", types.DefaultUserServer),
		Sender: types.NewJID("reactor", types.DefaultUserServer),
	}}

	if line, ok := statusProtocolReactionDiagnostic(info, message); ok || line != "" {
		t.Fatalf("ordinary reaction emitted diagnostic: %q", line)
	}
}

func TestActionJIDAndBridgeSourceContracts(t *testing.T) {
	if _, err := parseActionJID(""); err == nil {
		t.Fatal("empty action JID was accepted")
	}
}

func TestSafeDownloadTargetRejectsUnsafePathsAndPreservesNestedPaths(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	for _, target := range []string{"../escape", "/absolute", "", "."} {
		if _, err := safeDownloadTarget(root, target); err == nil {
			t.Fatalf("unsafe target %q was accepted", target)
		}
	}
	if got, err := safeDownloadTarget(root, "nested/file.jpg"); err != nil || got != filepath.Join(root, "nested/file.jpg") {
		t.Fatalf("nested target = %q, %v", got, err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "escape")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape/file.jpg", []byte("data")); err == nil {
		t.Fatal("symlink escape was accepted")
	}
}

func TestWriteDownloadRejectsSymlinkedRootsAndFinalPaths(t *testing.T) {
	root, outside := t.TempDir(), t.TempDir()
	link := filepath.Join(t.TempDir(), "media")
	if err := os.Symlink(outside, link); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(link, "file.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked root was accepted")
	}
	if err := writeDownload(root, "nested/file.jpg", []byte("data")); err != nil {
		t.Fatal(err)
	}
	if got, err := os.ReadFile(filepath.Join(root, "nested/file.jpg")); err != nil || string(got) != "data" {
		t.Fatalf("download = %q, %v", got, err)
	}
	if err := os.Symlink(filepath.Join(outside, "escape.jpg"), filepath.Join(root, "escape.jpg")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked final path was replaced")
	}
}

func TestWriteDownloadNeverAcceptsExistingOrPartialRegularFilesAndCleansFailedWrites(t *testing.T) {
	root := t.TempDir()
	destination := filepath.Join(root, "partial.jpg")
	if err := os.WriteFile(destination, []byte("partial"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "partial.jpg", []byte("complete")); err == nil {
		t.Fatal("existing partial regular file was accepted")
	}
	got, err := os.ReadFile(destination)
	if err != nil || string(got) != "partial" {
		t.Fatalf("existing file was changed: %q, %v", got, err)
	}
	if _, err := os.Stat(filepath.Join(root, ".partial.jpg.part")); !os.IsNotExist(err) {
		t.Fatalf("rename failure left a stale temporary file: %v", err)
	}
	if err := os.Remove(destination); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "partial.jpg", []byte("retry")); err != nil {
		t.Fatalf("retry after destination removal failed: %v", err)
	}
}

func TestWriteDownloadCleansTemporaryFileAfterWriteFailure(t *testing.T) {
	root := t.TempDir()
	writeFailure := errors.New("injected write failure")

	err := writeDownloadWithWriter(root, "failed.jpg", []byte("data"), func(*os.File, []byte) (int, error) {
		return 0, writeFailure
	})

	if !errors.Is(err, writeFailure) {
		t.Fatalf("write error = %v, want injected failure", err)
	}
	if _, err := os.Stat(filepath.Join(root, ".failed.jpg.part")); !os.IsNotExist(err) {
		t.Fatalf("failed write left a stale temporary file: %v", err)
	}
}

func TestBuildFileMessageMapsEveryFileKind(t *testing.T) {
	t.Parallel()

	directory := t.TempDir()
	contextInfo := &waE2E.ContextInfo{StanzaID: stringPointer("quoted-message")}
	uploaded := whatsmeow.UploadResponse{
		URL:           "https://upload.example/media",
		DirectPath:    "/v/t62.7118-24/media",
		MediaKey:      []byte("media-key"),
		FileEncSHA256: []byte("encrypted-hash"),
		FileSHA256:    []byte("plain-hash"),
	}

	testCases := []struct {
		name      string
		kind      uint8
		extension string
		mediaType whatsmeow.MediaType
		assert    func(*testing.T, *waE2E.Message)
	}{
		{
			name:      "image",
			kind:      FileTypeImage,
			extension: ".png",
			mediaType: whatsmeow.MediaImage,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.ImageMessage == nil || message.ImageMessage.Caption == nil {
					t.Fatal("expected image message with caption")
				}
				assertMediaFields(t, message.ImageMessage.GetURL(), message.ImageMessage.GetDirectPath(), message.ImageMessage.GetMimetype(), message.ImageMessage.GetFileLength(), message.ImageMessage.GetContextInfo(), message.ImageMessage.GetMediaKey(), message.ImageMessage.GetFileSHA256(), message.ImageMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "video",
			kind:      FileTypeVideo,
			extension: ".mp4",
			mediaType: whatsmeow.MediaVideo,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.VideoMessage == nil || message.VideoMessage.Caption == nil {
					t.Fatal("expected video message with caption")
				}
				assertMediaFields(t, message.VideoMessage.GetURL(), message.VideoMessage.GetDirectPath(), message.VideoMessage.GetMimetype(), message.VideoMessage.GetFileLength(), message.VideoMessage.GetContextInfo(), message.VideoMessage.GetMediaKey(), message.VideoMessage.GetFileSHA256(), message.VideoMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "audio",
			kind:      FileTypeAudio,
			extension: ".ogg",
			mediaType: whatsmeow.MediaAudio,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.AudioMessage == nil {
					t.Fatal("expected audio message")
				}
				assertMediaFields(t, message.AudioMessage.GetURL(), message.AudioMessage.GetDirectPath(), message.AudioMessage.GetMimetype(), message.AudioMessage.GetFileLength(), message.AudioMessage.GetContextInfo(), message.AudioMessage.GetMediaKey(), message.AudioMessage.GetFileSHA256(), message.AudioMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "document",
			kind:      FileTypeDocument,
			extension: ".txt",
			mediaType: whatsmeow.MediaDocument,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.DocumentMessage == nil || message.DocumentMessage.Caption == nil {
					t.Fatal("expected document message with caption")
				}
				assertMediaFields(t, message.DocumentMessage.GetURL(), message.DocumentMessage.GetDirectPath(), message.DocumentMessage.GetMimetype(), message.DocumentMessage.GetFileLength(), message.DocumentMessage.GetContextInfo(), message.DocumentMessage.GetMediaKey(), message.DocumentMessage.GetFileSHA256(), message.DocumentMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "sticker",
			kind:      FileTypeSticker,
			extension: ".webp",
			mediaType: whatsmeow.MediaImage,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.StickerMessage == nil {
					t.Fatal("expected sticker message")
				}
				assertMediaFields(t, message.StickerMessage.GetURL(), message.StickerMessage.GetDirectPath(), message.StickerMessage.GetMimetype(), message.StickerMessage.GetFileLength(), message.StickerMessage.GetContextInfo(), message.StickerMessage.GetMediaKey(), message.StickerMessage.GetFileSHA256(), message.StickerMessage.GetFileEncSHA256(), uploaded)
			},
		},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			path := filepath.Join(directory, "attachment"+testCase.extension)
			data := []byte(testCase.name + " payload")
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatal(err)
			}

			caption := "caption"
			var uploadedAs whatsmeow.MediaType
			message, err := buildFileMessage(context.Background(), testCase.kind, path, &caption, contextInfo, func(_ context.Context, actualData []byte, mediaType whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
				if string(actualData) != string(data) {
					t.Fatalf("uploaded data = %q, want %q", actualData, data)
				}
				uploadedAs = mediaType
				return uploaded, nil
			})
			if err != nil {
				t.Fatal(err)
			}
			if uploadedAs != testCase.mediaType {
				t.Fatalf("upload media type = %q, want %q", uploadedAs, testCase.mediaType)
			}
			testCase.assert(t, message)
			if actualCaption := messageCaption(message); actualCaption != nil && *actualCaption != caption {
				t.Fatalf("caption = %q, want %q", *actualCaption, caption)
			}
		})
	}
}

func TestBuildFileMessageKeepsNilCaptionNil(t *testing.T) {
	t.Parallel()

	for _, testCase := range []struct {
		name string
		kind uint8
		ext  string
	}{
		{name: "image", kind: FileTypeImage, ext: ".png"},
		{name: "video", kind: FileTypeVideo, ext: ".mp4"},
		{name: "document", kind: FileTypeDocument, ext: ".txt"},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "attachment"+testCase.ext)
			if err := os.WriteFile(path, []byte("media payload"), 0o600); err != nil {
				t.Fatal(err)
			}

			message, err := buildFileMessage(context.Background(), testCase.kind, path, nil, &waE2E.ContextInfo{}, func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
				return whatsmeow.UploadResponse{}, nil
			})
			if err != nil {
				t.Fatal(err)
			}
			if caption := messageCaption(message); caption != nil {
				t.Fatalf("caption = %q, want nil", *caption)
			}
		})
	}
}

func TestQuotedContextInfoPreservesOriginalChatForStatusAndOrdinaryReplies(t *testing.T) {
	status := quotedContextInfo("status-id", "alice@s.whatsapp.net", "status@broadcast")
	if status.GetStanzaID() != "status-id" || status.GetParticipant() != "alice@s.whatsapp.net" || status.GetRemoteJID() != "status@broadcast" {
		t.Fatalf("status quote context = %+v, want status target attribution", status)
	}

	ordinary := quotedContextInfo("message-id", "bob@s.whatsapp.net", "bob@s.whatsapp.net")
	if ordinary.GetRemoteJID() != "bob@s.whatsapp.net" {
		t.Fatalf("ordinary quote remote JID = %q, want original chat", ordinary.GetRemoteJID())
	}
}

func TestReactionRequestSeparatesTargetFromDestination(t *testing.T) {
	target := types.NewJID("status", types.BroadcastServer)
	destination := types.NewJID("alice", types.DefaultUserServer)
	sender := types.NewJID("alice", types.DefaultUserServer)

	request, err := newReactionRequest(target, destination, sender, "status-id", "❤️")
	if err != nil {
		t.Fatal(err)
	}
	if request.target != target || request.destination != destination || request.sender != sender {
		t.Fatalf("reaction request = %+v, want distinct status target and inbox destination", request)
	}

	message, err := buildOrdinaryReaction(&whatsmeow.Client{}, request.target, request.sender, request.id, request.reaction)
	if err != nil {
		t.Fatal(err)
	}
	if key := message.GetReactionMessage().GetKey(); key.GetRemoteJID() != target.String() || key.GetParticipant() != sender.String() || key.GetID() != "status-id" {
		t.Fatalf("reaction key = %+v, want status target key", key)
	}
}

func assertMediaFields(t *testing.T, url, directPath, mimetype string, length uint64, contextInfo *waE2E.ContextInfo, mediaKey, fileSHA256, fileEncSHA256 []byte, uploaded whatsmeow.UploadResponse) {
	t.Helper()
	if url != uploaded.URL || directPath != uploaded.DirectPath {
		t.Fatalf("upload metadata = (%q, %q), want (%q, %q)", url, directPath, uploaded.URL, uploaded.DirectPath)
	}
	if mimetype == "" {
		t.Fatal("expected MIME type")
	}
	if length == 0 {
		t.Fatal("expected non-zero file length")
	}
	if string(mediaKey) != string(uploaded.MediaKey) || string(fileSHA256) != string(uploaded.FileSHA256) || string(fileEncSHA256) != string(uploaded.FileEncSHA256) {
		t.Fatal("expected upload hashes and media key to be preserved")
	}
	if contextInfo == nil || contextInfo.GetStanzaID() != "quoted-message" {
		t.Fatalf("context info = %#v, want quoted-message", contextInfo)
	}
}

func stringPointer(value string) *string {
	return &value
}

func messageCaption(message *waE2E.Message) *string {
	switch {
	case message.ImageMessage != nil:
		return message.ImageMessage.Caption
	case message.VideoMessage != nil:
		return message.VideoMessage.Caption
	case message.DocumentMessage != nil:
		return message.DocumentMessage.Caption
	default:
		return nil
	}
}

func int64Pointer(value int64) *int64 { return &value }
