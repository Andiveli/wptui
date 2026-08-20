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
		{name: "privacy unauthorized", jid: "1@s.whatsapp.net", lookupErr: whatsmeow.ErrProfilePictureUnauthorized, want: profilePictureStatusUnauthorized},
		{name: "picture not set", jid: "1@s.whatsapp.net", lookupErr: whatsmeow.ErrProfilePictureNotSet, want: profilePictureStatusNotSet},
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
			outcome := fetchProfilePicture(context.Background(), testCase.jid, profilePictureTarget{}, lookup, func(context.Context, string, int64) ([]byte, error) {
				return testCase.data, testCase.downloadErr
			})
			if outcome.status != testCase.want {
				t.Fatalf("status = %d, want %d", outcome.status, testCase.want)
			}
		})
	}
}

func TestFetchProfilePictureBuildsTypedWhatsmeowParams(t *testing.T) {
	parent := types.JID{User: "parent", Server: types.GroupServer}
	cases := []struct {
		name          string
		jid           string
		target        profilePictureTarget
		wantCommunity bool
		wantCommonGID types.JID
	}{
		{name: "community root", jid: "root@g.us", target: profilePictureTarget{IsCommunity: true}, wantCommunity: true},
		{name: "available subgroup with parent access", jid: "child@g.us", target: profilePictureTarget{CommonGID: parent}, wantCommonGID: parent},
		{name: "normal contact", jid: "123@s.whatsapp.net", target: profilePictureTarget{}},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			validImage := profilePicturePNG(t)
			var gotJID types.JID
			var gotParams whatsmeow.GetProfilePictureParams
			outcome := fetchProfilePicture(context.Background(), testCase.jid, testCase.target,
				func(_ context.Context, jid types.JID, params *whatsmeow.GetProfilePictureParams) (*types.ProfilePictureInfo, error) {
					gotJID = jid
					gotParams = *params
					return &types.ProfilePictureInfo{URL: "url"}, nil
				},
				func(context.Context, string, int64) ([]byte, error) { return validImage, nil },
			)
			if outcome.status != profilePictureStatusAvailable {
				t.Fatalf("status = %d, want available", outcome.status)
			}
			if gotJID.String() != testCase.jid {
				t.Fatalf("JID = %s, want %s", gotJID, testCase.jid)
			}
			if !gotParams.Preview || gotParams.IsCommunity != testCase.wantCommunity || gotParams.CommonGID != testCase.wantCommonGID {
				t.Fatalf("params = %#v, want Preview=true IsCommunity=%t CommonGID=%s", gotParams, testCase.wantCommunity, testCase.wantCommonGID)
			}
		})
	}
}
