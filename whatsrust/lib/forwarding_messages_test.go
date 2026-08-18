package main

import (
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestPrepareForwardMessagePreservesTextAndFilePayloads(t *testing.T) {
	text := &waE2E.Message{Conversation: stringPointer("hello")}
	forwarded, err := prepareForwardMessage(text, false)
	if err != nil || forwarded.GetExtendedTextMessage().GetText() != "hello" || !forwarded.GetExtendedTextMessage().GetContextInfo().GetIsForwarded() {
		t.Fatalf("text forwarding envelope = %#v, error = %v", forwarded, err)
	}
	if text.GetConversation() != "hello" || text.GetExtendedTextMessage() != nil {
		t.Fatalf("source message was mutated: %#v", text)
	}
	media := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{URL: stringPointer("https://media.example"), DirectPath: stringPointer("/media")}}
	forwarded, err = prepareForwardMessage(media, false)
	if err != nil || forwarded.GetImageMessage().GetDirectPath() != "/media" || !forwarded.GetImageMessage().GetContextInfo().GetIsForwarded() {
		t.Fatalf("file forwarding envelope = %#v, error = %v", forwarded, err)
	}
}

func TestPrepareForwardMessageAppliesMobileForwardingContextSemantics(t *testing.T) {
	stanzaID := "quoted-message"
	score := uint32(4)
	other := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("other person's message"), ContextInfo: &waE2E.ContextInfo{StanzaID: &stanzaID, ForwardingScore: &score, IsForwarded: proto.Bool(true)}}}
	forwarded, err := prepareForwardMessage(other, false)
	if err != nil {
		t.Fatal(err)
	}
	context := forwarded.GetExtendedTextMessage().GetContextInfo()
	if !context.GetIsForwarded() || context.GetForwardingScore() != 5 || context.GetStanzaID() != stanzaID {
		t.Fatalf("other sender context = %#v", context)
	}
	repeated, err := prepareForwardMessage(forwarded, false)
	if err != nil || !repeated.GetExtendedTextMessage().GetContextInfo().GetIsForwarded() || repeated.GetExtendedTextMessage().GetContextInfo().GetForwardingScore() != 5 || repeated.GetExtendedTextMessage().GetContextInfo().GetStanzaID() != stanzaID {
		t.Fatalf("capped forwarding context = %#v, error = %v", repeated, err)
	}
	own := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{DirectPath: stringPointer("/media"), ContextInfo: &waE2E.ContextInfo{StanzaID: &stanzaID, ForwardingScore: &score, IsForwarded: proto.Bool(true)}}}
	forwarded, err = prepareForwardMessage(own, true)
	context = forwarded.GetImageMessage().GetContextInfo()
	if err != nil || context.GetIsForwarded() || context.GetForwardingScore() != 0 || context.GetStanzaID() != stanzaID || forwarded.GetImageMessage().GetDirectPath() != "/media" {
		t.Fatalf("own sender context = %#v, message = %#v, error = %v", context, forwarded, err)
	}
}

func TestSourceOwnedByCurrentUserUsesMessageMetadataAndClientIdentity(t *testing.T) {
	self := types.NewJID("self", types.DefaultUserServer)
	other := types.NewJID("other", types.DefaultUserServer)
	if !sourceOwnedByCurrentUser(true, other, self) || !sourceOwnedByCurrentUser(false, self, self) || sourceOwnedByCurrentUser(false, other, self) {
		t.Fatal("source ownership did not preserve metadata and client identity")
	}
}

func TestForwardSourceBytesSurviveCacheReset(t *testing.T) {
	for _, message := range []*waE2E.Message{{Conversation: stringPointer("historical")}, {ImageMessage: &waE2E.ImageMessage{DirectPath: stringPointer("/media")}}} {
		raw, err := proto.Marshal(message)
		if err != nil {
			t.Fatal(err)
		}
		resetForwardedSourcesForTest()
		restored, reason := forwardSourceFromBytes(raw)
		if reason != forwardFailureNone || !proto.Equal(restored, message) {
			t.Fatalf("source recovery = %#v, %v", restored, reason)
		}
	}
}

func TestMarshalForwardSourceExcludesViewOnce(t *testing.T) {
	message := &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{Conversation: stringPointer("private")}}}
	raw, err := marshalForwardSource(message)
	if err != nil || len(raw) != 0 {
		t.Fatalf("view-once source bytes = %v, error = %v", raw, err)
	}
}

func TestForwardMessagePreparationAndDecodingReportFailures(t *testing.T) {
	if _, err := prepareForwardMessage(nil, false); err == nil {
		t.Fatal("nil source should fail")
	}
	if _, err := prepareForwardMessage(&waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{}}, false); err == nil {
		t.Fatal("unsupported content should fail")
	}
	for _, raw := range [][]byte{nil, []byte("invalid protobuf")} {
		if message, reason := forwardSourceFromBytes(raw); message != nil || reason != forwardFailureSourceUnavailable {
			t.Fatalf("source failure = %#v, %v", message, reason)
		}
	}
}
