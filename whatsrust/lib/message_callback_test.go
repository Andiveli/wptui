package main

import (
	"context"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestMessageCallbackMetadataFromPreservesCallbackFields(t *testing.T) {
	info := types.MessageInfo{
		MessageSource: types.MessageSource{
			Chat:     types.NewJID("chat", "example.test"),
			Sender:   types.NewJID("sender", "example.test"),
			IsFromMe: true,
		},
		ID:        "message-id",
		PushName:  "Message Push Name",
		Timestamp: time.Unix(123, 456),
	}
	forwarded := true
	message := &waE2E.Message{
		ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			ContextInfo: &waE2E.ContextInfo{IsForwarded: &forwarded, ForwardingScore: proto.Uint32(4)},
		},
	}

	metadata := messageCallbackMetadataFrom(info, message)
	if metadata.id != info.ID || metadata.chat != info.Chat || metadata.sender != info.Sender {
		t.Fatalf("callback identity metadata changed: %#v", metadata)
	}
	if metadata.timestamp != 123 || !metadata.isFromMe || metadata.pushName != info.PushName {
		t.Fatalf("callback scalar metadata changed: %#v", metadata)
	}
	if metadata.forwarding != (forwardingState{isForwarded: true, score: 4}) {
		t.Fatalf("callback forwarding metadata changed: %#v", metadata.forwarding)
	}
}

func TestMessageCallbackCarriesSemanticSelfMentionForPNAndLID(t *testing.T) {
	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	previousClient := client
	client = &whatsmeow.Client{Store: &store.Device{
		ID:  &pn,
		LID: lid,
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	t.Cleanup(func() { client = previousClient })

	message := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
		ContextInfo: &waE2E.ContextInfo{MentionedJID: []string{lid.String()}},
	}}
	info := types.MessageInfo{Timestamp: time.Unix(1, 0)}
	metadata := messageCallbackMetadataFrom(info, message)
	if !metadata.mentionsSelf {
		t.Fatal("mapped LID mention must be marked as a self mention")
	}
	callback := beginMessageCallback(info, message, nil)
	defer callback.close()
	if !bool(callback.info.mentionsSelf) {
		t.Fatal("self mention semantic flag was lost at the C callback boundary")
	}
}

func TestMessageCallbackSelfMentionIdentityVariantsFailClosed(t *testing.T) {
	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	previousClient := client
	client = &whatsmeow.Client{Store: &store.Device{
		ID:  &pn,
		LID: lid,
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}}
	t.Cleanup(func() { client = previousClient })

	tests := []struct {
		name      string
		mentioned string
		want      bool
	}{
		{name: "PN", mentioned: pn.String(), want: true},
		{name: "AD normalized PN", mentioned: types.NewADJID("123", 0, 7).String(), want: true},
		{name: "mapped LID", mentioned: lid.String(), want: true},
		{name: "unknown LID", mentioned: types.NewJID("999", types.HiddenUserServer).String(), want: false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			message := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
				ContextInfo: &waE2E.ContextInfo{MentionedJID: []string{tt.mentioned}},
			}}
			if got := messageMentionsSelf(message); got != tt.want {
				t.Fatalf("messageMentionsSelf(%q) = %v, want %v", tt.mentioned, got, tt.want)
			}
		})
	}
}

func TestBeginMessageCallbackPreservesPushNameAcrossCMetadata(t *testing.T) {
	info := types.MessageInfo{PushName: "WhatsApp Profile"}
	callback := beginMessageCallback(info, &waE2E.Message{}, nil)
	defer callback.close()
	if got := messageCallbackPushName(callback); got != info.PushName {
		t.Fatalf("callback push name = %q, want %q", got, info.PushName)
	}
}

func TestSelfMentionUsesAuthenticatedPushNameLearnedFromOwnCallback(t *testing.T) {
	previousClient := client
	t.Cleanup(func() {
		client = previousClient
		clearAuthenticatedPushNameCache()
	})

	pn := types.NewJID("593995682425", types.DefaultUserServer)
	client = whatsmeow.NewClient(&store.Device{
		ID: &pn,
		Contacts: mentionDirectContactStore{direct: map[types.JID]types.ContactInfo{
			pn: {FullName: "+593 99 568 2425", FirstName: "+593 99 568 2425"},
		}},
	}, nil)

	callback := beginMessageCallback(types.MessageInfo{
		MessageSource: types.MessageSource{Sender: pn, IsFromMe: true},
		PushName:      "SAMA3L",
	}, &waE2E.Message{}, nil)
	callback.close()

	mentioned := []string{pn.String()}
	entries := mentionEntriesForGroup(context.Background(), types.NewJID("12345", types.GroupServer), mentioned...)
	got := replaceMentionedNames("hello @593995682425", mentioned, entries)
	if got != "hello @SAMA3L" {
		t.Fatalf("self mention rendering = %q, want authenticated profile name", got)
	}
}

func TestOtherUsersPushNameNeverContaminatesAuthenticatedSelfCache(t *testing.T) {
	previousClient := client
	t.Cleanup(func() {
		client = previousClient
		clearAuthenticatedPushNameCache()
	})

	self := types.NewJID("593995682425", types.DefaultUserServer)
	bryan := types.NewJID("15551234567", types.DefaultUserServer)
	client = whatsmeow.NewClient(&store.Device{ID: &self}, nil)

	callback := beginMessageCallback(types.MessageInfo{
		MessageSource: types.MessageSource{Sender: bryan, IsFromMe: false},
		PushName:      "Bryan",
	}, &waE2E.Message{}, nil)
	callback.close()
	callback = beginMessageCallback(types.MessageInfo{
		MessageSource: types.MessageSource{Sender: bryan, IsFromMe: true},
		PushName:      "Forged Bryan",
	}, &waE2E.Message{}, nil)
	callback.close()

	if got := selfDisplayName(context.Background(), client); got != "" {
		t.Fatalf("other user's push name contaminated self cache: %q", got)
	}
}

func TestAuthenticatedPushNameCacheIsolatedByAccountIdentity(t *testing.T) {
	previousClient := client
	t.Cleanup(func() {
		client = previousClient
		clearAuthenticatedPushNameCache()
	})

	first := types.NewJID("111", types.DefaultUserServer)
	client = whatsmeow.NewClient(&store.Device{ID: &first}, nil)
	callback := beginMessageCallback(types.MessageInfo{
		MessageSource: types.MessageSource{Sender: first, IsFromMe: true},
		PushName:      "First Account",
	}, &waE2E.Message{}, nil)
	callback.close()

	second := types.NewJID("222", types.DefaultUserServer)
	client = whatsmeow.NewClient(&store.Device{ID: &second}, nil)
	if got := selfDisplayName(context.Background(), client); got != "" {
		t.Fatalf("push name from previous account leaked into current account: %q", got)
	}
}

func TestBeginMessageCallbackSerializesCallbacks(t *testing.T) {
	info := types.MessageInfo{Timestamp: time.Unix(1, 0)}
	first := beginMessageCallback(info, &waE2E.Message{}, []byte("source"))
	acquired := make(chan struct{})
	done := make(chan *messageCallback)
	go func() {
		callback := beginMessageCallback(info, &waE2E.Message{}, nil)
		close(acquired)
		done <- callback
	}()

	select {
	case <-acquired:
		t.Fatal("second callback acquired the callback lock early")
	case <-time.After(10 * time.Millisecond):
	}
	first.close()

	select {
	case second := <-done:
		second.close()
	case <-time.After(time.Second):
		t.Fatal("second callback did not acquire the callback lock")
	}
}
