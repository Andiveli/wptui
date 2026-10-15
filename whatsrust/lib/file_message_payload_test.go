package main

import (
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func TestFileMessageCaptionSkipsAllocationForEmptyCaptions(t *testing.T) {
	if caption := fileMessageCaption(""); caption != nil {
		t.Fatal("empty file captions must not allocate C memory")
	}
}

func TestQuotedMediaCallbackOwnsQuoteIDUntilCallbackClose(t *testing.T) {
	callback := beginMessageCallback(types.MessageInfo{}, &waE2E.Message{}, nil)
	callback.setQuoteIDFromContext(&waE2E.ContextInfo{StanzaID: proto.String("quoted-message")})
	if callback.info.quoteID == nil {
		t.Fatal("quoted media must transfer its quote ID to the callback owner")
	}

	callback.close()
	if callback.info.quoteID != nil {
		t.Fatal("callback-owned quote ID was not released after the synchronous callback returned")
	}
}

func TestMediaCallbackClearsQuoteIDWhenMediaIsNotQuoted(t *testing.T) {
	callback := beginMessageCallback(types.MessageInfo{}, &waE2E.Message{}, nil)
	defer callback.close()
	callback.setQuoteIDFromContext(&waE2E.ContextInfo{StanzaID: proto.String("quoted-message")})
	callback.setQuoteIDFromContext(nil)
	if callback.info.quoteID != nil {
		t.Fatal("unquoted media must not inherit a prior media quote ID")
	}
}

func TestMediaEmittersUseTheCallbackQuoteIDOwner(t *testing.T) {
	source := string(mustRead(t, "file_message_payload.go"))
	for _, emitter := range []string{
		"emitImageMessage(callback *messageCallback",
		"emitVideoMessage(callback *messageCallback",
		"emitAudioMessage(callback *messageCallback",
		"emitDocumentMessage(callback *messageCallback",
		"emitStickerMessage(callback *messageCallback",
		"callback.setQuoteIDFromContext(",
	} {
		if !strings.Contains(source, emitter) {
			t.Fatalf("media callback quote ID ownership is missing: %q", emitter)
		}
	}
}
