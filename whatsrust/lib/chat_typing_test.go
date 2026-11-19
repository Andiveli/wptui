package main

import (
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestIncomingChatPresenceIsRouted(t *testing.T) {
	source, err := os.ReadFile("event_dispatch.go")
	if err != nil {
		t.Fatal(err)
	}
	body, ok := extractFunctionBody(string(source), "func dispatchEventFamily(")
	if !ok || !strings.Contains(body, "case *events.ChatPresence:\n\t\tdispatchChatTyping(evt, client)") {
		t.Fatal("incoming ChatPresence is not routed to the composing handler")
	}
}

func TestDirectChatTypingUsesSameConversationForLIDAndPhone(t *testing.T) {
	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	client := &whatsmeow.Client{Store: &store.Device{LIDs: participantIdentityLIDStore{
		pnByLID: map[types.JID]types.JID{lid: pn},
	}}}
	for _, tc := range []struct {
		name      string
		jid       types.JID
		state     types.ChatPresence
		composing bool
	}{
		{"composing from LID", lid, types.ChatPresenceComposing, true},
		{"pause from phone", pn, types.ChatPresencePaused, false},
		{"pause from LID", lid, types.ChatPresencePaused, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got, ok := chatTypingFromPresence(client, &events.ChatPresence{
				MessageSource: types.MessageSource{Chat: tc.jid, Sender: tc.jid},
				State:         tc.state,
			})
			want := chatTypingEvent{pn.String(), pn.String(), tc.composing}
			if !ok || got != want {
				t.Fatalf("got (%+v, %t), want (%+v, true)", got, ok, want)
			}
		})
	}
}

func TestChatTypingFromPresence(t *testing.T) {
	direct := types.NewJID("123", types.DefaultUserServer)
	group := types.NewJID("456", types.GroupServer)
	sender := types.NewADJID("789", 0, 1)
	for _, tc := range []struct {
		name         string
		chat, sender types.JID
		state        types.ChatPresence
		media        types.ChatPresenceMedia
		own, ok      bool
		want         chatTypingEvent
	}{
		{"direct", direct, direct, types.ChatPresenceComposing, "", false, true, chatTypingEvent{"123@s.whatsapp.net", "123@s.whatsapp.net", true}},
		{"group participant", group, sender, types.ChatPresenceComposing, "", false, true, chatTypingEvent{"456@g.us", "789@s.whatsapp.net", true}},
		{"paused", group, sender, types.ChatPresencePaused, "", false, true, chatTypingEvent{"456@g.us", "789@s.whatsapp.net", false}},
		{"recording is not typing", group, sender, types.ChatPresenceComposing, types.ChatPresenceMediaAudio, false, true, chatTypingEvent{"456@g.us", "789@s.whatsapp.net", false}},
		{"own device", group, sender, types.ChatPresenceComposing, "", true, false, chatTypingEvent{}},
		{"missing sender", group, types.EmptyJID, types.ChatPresenceComposing, "", false, false, chatTypingEvent{}},
		{"unknown state", group, sender, "unknown", "", false, false, chatTypingEvent{}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			event := &events.ChatPresence{
				MessageSource: types.MessageSource{Chat: tc.chat, Sender: tc.sender, IsFromMe: tc.own},
				State:         tc.state,
				Media:         tc.media,
			}
			got, ok := chatTypingFromPresence(nil, event)
			if ok != tc.ok || got != tc.want {
				t.Fatalf("got (%+v, %t), want (%+v, %t)", got, ok, tc.want, tc.ok)
			}
		})
	}
}
