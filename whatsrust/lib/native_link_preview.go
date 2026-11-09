package main

import (
	"strings"
	"unicode"
	"unicode/utf8"

	"go.mau.fi/whatsmeow/proto/waE2E"
)

const nativeMatchedTextLimit = 8192
const nativeTitleLimit = 2048
const nativeDescriptionLimit = 8192
const nativeThumbnailLimit = 256 * 1024

type nativeLinkPreview struct {
	matchedText   string
	title         string
	description   string
	jpegThumbnail []byte
}

func safeNativeText(text string, limit int) bool {
	if len(text) > limit || !utf8.ValidString(text) {
		return false
	}
	for _, char := range text {
		if unicode.IsControl(char) || char == '\u061c' || char == '\u200e' || char == '\u200f' || (char >= '\u202a' && char <= '\u202e') || (char >= '\u2066' && char <= '\u2069') {
			return false
		}
	}
	return true
}

// Only embedded bytes are transported. Direct paths never trigger a download.
func incomingNativeLinkPreview(ext *waE2E.ExtendedTextMessage, fromMe bool) *nativeLinkPreview {
	if fromMe || ext == nil || ext.GetMatchedText() == "" ||
		!safeNativeText(ext.GetMatchedText(), nativeMatchedTextLimit) ||
		!safeNativeText(ext.GetTitle(), nativeTitleLimit) ||
		!safeNativeText(ext.GetDescription(), nativeDescriptionLimit) {
		return nil
	}
	preview := &nativeLinkPreview{
		matchedText: strings.Clone(ext.GetMatchedText()),
		title:       strings.Clone(ext.GetTitle()),
		description: strings.Clone(ext.GetDescription()),
	}
	if len(ext.GetJPEGThumbnail()) <= nativeThumbnailLimit {
		preview.jpegThumbnail = append([]byte(nil), ext.GetJPEGThumbnail()...)
	}
	return preview
}
