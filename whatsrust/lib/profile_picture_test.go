package main

import (
	"context"
	"errors"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

func TestFetchProfilePictureRecoverableOutcomes(t *testing.T) {
	validImage := profilePicturePNG(t)
	metadataError := errors.New("metadata failed")
	downloadError := errors.New("download failed")
	cases := []struct {
		name        string
		jid         string
		info        *types.ProfilePictureInfo
		lookupErr   error
		data        []byte
		downloadErr error
		lookupNil   bool
		want        uint8
	}{
		{name: "empty JID", jid: "", want: profilePictureStatusInvalidJID},
		{name: "malformed JID", jid: "a@b@c", want: profilePictureStatusInvalidJID},
		{name: "unsupported JID", jid: "status@broadcast", want: profilePictureStatusInvalidJID},
		{name: "client unavailable", jid: "1@s.whatsapp.net", lookupNil: true, want: profilePictureStatusClientUnavailable},
		{name: "privacy unauthorized", jid: "1@s.whatsapp.net", lookupErr: whatsmeow.ErrProfilePictureUnauthorized, want: profilePictureStatusUnavailable},
		{name: "picture not set", jid: "1@s.whatsapp.net", lookupErr: whatsmeow.ErrProfilePictureNotSet, want: profilePictureStatusUnavailable},
		{name: "cancelled metadata", jid: "1@s.whatsapp.net", lookupErr: context.Canceled, want: profilePictureStatusCancelled},
		{name: "metadata failure", jid: "1@s.whatsapp.net", lookupErr: metadataError, want: profilePictureStatusMetadataFailed},
		{name: "nil metadata", jid: "1@s.whatsapp.net", want: profilePictureStatusMetadataFailed},
		{name: "empty URL", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{ID: "id", Type: "preview"}, want: profilePictureStatusEmptyURL},
		{name: "network failure", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, downloadErr: downloadError, want: profilePictureStatusDownloadFailed},
		{name: "cancelled download", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, downloadErr: context.DeadlineExceeded, want: profilePictureStatusCancelled},
		{name: "oversized download", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, downloadErr: errProfilePictureOversized, want: profilePictureStatusOversized},
		{name: "oversized returned payload", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, data: make([]byte, profilePictureMaxSize+1), want: profilePictureStatusOversized},
		{name: "empty image", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, want: profilePictureStatusInvalidImage},
		{name: "invalid image", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, data: []byte("not an image"), want: profilePictureStatusInvalidImage},
		{name: "valid contact preview", jid: "1@s.whatsapp.net", info: &types.ProfilePictureInfo{URL: "url"}, data: validImage, want: profilePictureStatusAvailable},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			var lookup profilePictureLookup
			if !testCase.lookupNil {
				lookup = func(context.Context, types.JID, *whatsmeow.GetProfilePictureParams) (*types.ProfilePictureInfo, error) {
					return testCase.info, testCase.lookupErr
				}
			}
			outcome := fetchProfilePicture(context.Background(), testCase.jid, lookup, func(context.Context, string, int64) ([]byte, error) {
				return testCase.data, testCase.downloadErr
			})
			if outcome.status != testCase.want {
				t.Fatalf("status = %d, want %d", outcome.status, testCase.want)
			}
		})
	}
}
