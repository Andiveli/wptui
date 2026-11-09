package main

import (
	"bytes"
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

// This probes the existing callback-owned text seam, not a second Go cache.
// nativePreview is an optional owned adjunct to textCallbackOutput, mirrored
// by the C text payload. It must not change MessageInfo or ordinary text.
func TestNativePreviewExtendedTextRetainsSuppliedFields(t *testing.T) {
	previous := observeTextCallback
	t.Cleanup(func() { observeTextCallback = previous })
	var got textCallbackOutput
	observeTextCallback = func(output textCallbackOutput) { got = output }

	body := "原文 https://example.test/article?q=%2F#part"
	thumbnail := []byte{0xff, 0xd8, 0xff, 0xd9} // opaque bytes, never decoded here
	HandleMessage(types.MessageInfo{ID: "native-preview"}, &waE2E.Message{
		ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			Text: proto.String(body), MatchedText: proto.String("https://example.test/article?q=%2F#part"),
			Title: proto.String("Native title"), Description: proto.String("Native description"),
			JPEGThumbnail: thumbnail,
			ContextInfo:   &waE2E.ContextInfo{StanzaID: proto.String("quoted")},
		},
	}, false)
	if got.text != body || got.quoteID != "quoted" || got.localSendID != 0 {
		t.Fatalf("text/quote/callback identity changed: %#v", got)
	}
	if got.nativePreview == nil {
		t.Fatal("incoming extended text discarded its native preview")
	}
	preview := got.nativePreview
	if preview.matchedText != "https://example.test/article?q=%2F#part" || preview.title != "Native title" || preview.description != "Native description" || !bytes.Equal(preview.jpegThumbnail, thumbnail) {
		t.Fatalf("native fields changed at callback: %#v", preview)
	}
	thumbnail[0] = 0
	if preview.jpegThumbnail[0] != 0xff {
		t.Fatal("callback preview retained borrowed protobuf thumbnail memory")
	}
}

func TestNativePreviewDoesNotLeakIntoOrdinaryOutgoingOrMissingMetadata(t *testing.T) {
	previous := observeTextCallback
	t.Cleanup(func() { observeTextCallback = previous })
	body := "https://example.test/article"
	withPreview := func() *waE2E.Message {
		return &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			Text: proto.String(body), MatchedText: proto.String(body), Title: proto.String("Native title"),
		}}
	}
	for _, test := range []struct {
		name       string
		message    *waE2E.Message
		fromMe     bool
		optimistic bool
	}{
		{name: "conversation", message: &waE2E.Message{Conversation: proto.String(body)}},
		{name: "extended without metadata", message: &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: proto.String(body)}}},
		{name: "own server echo", message: withPreview(), fromMe: true},
		{name: "optimistic own text", message: withPreview(), fromMe: true, optimistic: true},
	} {
		t.Run(test.name, func(t *testing.T) {
			var outputs []textCallbackOutput
			observeTextCallback = func(output textCallbackOutput) { outputs = append(outputs, output) }
			// Prime the seam first: a previous message must never contaminate the next.
			HandleMessage(types.MessageInfo{ID: "prime"}, withPreview(), false)
			info := types.MessageInfo{ID: "plain", MessageSource: types.MessageSource{IsFromMe: test.fromMe}}
			if test.optimistic {
				HandleOptimisticTextSent(42, info, test.message)
			} else {
				HandleMessage(info, test.message, false)
			}
			if len(outputs) != 2 {
				t.Fatalf("callback count = %d, want 2", len(outputs))
			}
			got := outputs[1]
			wantSendID := uint64(0)
			if test.optimistic {
				wantSendID = 42
			}
			if got.text != body || got.nativePreview != nil || got.localSendID != wantSendID || got.quoteID != "" || len(got.ranges) != 0 {
				t.Fatalf("legacy callback behavior changed: %#v", got)
			}
		})
	}
}

func TestNativePreviewWithoutEmbeddedThumbnailRemainsTextOnly(t *testing.T) {
	previous := observeTextCallback
	t.Cleanup(func() { observeTextCallback = previous })
	var got textCallbackOutput
	observeTextCallback = func(output textCallbackOutput) { got = output }
	body := "https://example.test/article"
	HandleMessage(types.MessageInfo{ID: "text-only-preview"}, &waE2E.Message{
		ExtendedTextMessage: &waE2E.ExtendedTextMessage{
			Text: proto.String(body), MatchedText: proto.String(body), Title: proto.String("Native title"),
			ThumbnailDirectPath: proto.String("/do-not-download"),
		},
	}, true)
	if got.text != body || got.nativePreview == nil || got.nativePreview.title != "Native title" || len(got.nativePreview.jpegThumbnail) != 0 {
		t.Fatalf("missing embedded image must preserve text-only metadata: %#v", got)
	}
}
