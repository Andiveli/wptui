package main

import (
	"bytes"
	"context"
	"image"
	"image/color"
	"image/png"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

func portraitSourcePNGFixture(t *testing.T, size int) []byte {
	t.Helper()
	picture := image.NewNRGBA(image.Rect(0, 0, size, size))
	for y := 0; y < size; y++ {
		for x := 0; x < size; x++ {
			picture.SetNRGBA(x, y, color.NRGBA{
				R: uint8((x / 8) % 2 * 192),
				G: uint8((y / 8) % 2 * 160),
				B: uint8((x + y) % 256),
				A: 255,
			})
		}
	}
	var encoded bytes.Buffer
	if err := png.Encode(&encoded, picture); err != nil {
		t.Fatal(err)
	}
	return encoded.Bytes()
}

func TestContactPortraitRequestsFullSourceAndPreservesGroupPreviews(t *testing.T) {
	preview := portraitSourcePNGFixture(t, 100)
	full := portraitSourcePNGFixture(t, 640)
	cases := []struct {
		name        string
		jid         string
		community   bool
		wantPreview bool
		wantSize    int
	}{
		{name: "phone contact image", jid: "15551234567@s.whatsapp.net", wantSize: 640},
		{name: "LID contact image", jid: "123456789@lid", wantSize: 640},
		{name: "ordinary group keeps preview", jid: "12345@g.us", wantPreview: true, wantSize: 100},
		{name: "community root keeps preview", jid: "12345@g.us", community: true, wantPreview: true, wantSize: 100},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			lookupCalls, downloadCalls := 0, 0
			var actual whatsmeow.GetProfilePictureParams
			lookup := func(_ context.Context, jid types.JID, params *whatsmeow.GetProfilePictureParams) (*types.ProfilePictureInfo, error) {
				lookupCalls++
				if params == nil || jid.String() != testCase.jid {
					t.Fatalf("unexpected lookup: jid=%s params=%#v", jid, params)
				}
				actual = *params
				variant := "image"
				if params.Preview {
					variant = "preview"
				}
				// The same ID deliberately identifies both source variants.
				return &types.ProfilePictureInfo{URL: "https://fixture.invalid/" + variant, ID: "same-photo-id", Type: variant}, nil
			}
			download := func(_ context.Context, url string, limit int64) ([]byte, error) {
				downloadCalls++
				if limit != profilePictureMaxSize {
					t.Fatalf("download limit = %d, want %d", limit, profilePictureMaxSize)
				}
				switch url {
				case "https://fixture.invalid/image":
					return bytes.Clone(full), nil
				case "https://fixture.invalid/preview":
					return bytes.Clone(preview), nil
				default:
					t.Fatalf("unexpected source URL: %q", url)
					return nil, nil
				}
			}
			var outcome profilePictureOutcome
			if testCase.community {
				outcome = fetchProfilePictureWithParams(context.Background(), testCase.jid, true, lookup, download)
			} else {
				outcome = fetchProfilePicture(context.Background(), testCase.jid, lookup, download)
			}
			if lookupCalls != 1 || downloadCalls != 1 {
				t.Errorf("lookup/download calls = %d/%d, want 1/1", lookupCalls, downloadCalls)
			}
			if actual.Preview != testCase.wantPreview || actual.IsCommunity != testCase.community || actual.CommonGID != (types.JID{}) {
				t.Errorf("source params = %#v, want Preview=%t IsCommunity=%t CommonGID=empty", actual, testCase.wantPreview, testCase.community)
			}
			wantData, wantType := full, "image"
			if testCase.wantPreview {
				wantData, wantType = preview, "preview"
			}
			if outcome.status != profilePictureStatusAvailable || outcome.pictureID != "same-photo-id" || outcome.pictureType != wantType {
				t.Errorf("metadata = status:%d id:%q type:%q, want available/same-photo-id/%s", outcome.status, outcome.pictureID, outcome.pictureType, wantType)
			}
			if !bytes.Equal(outcome.data, wantData) {
				t.Errorf("downloaded source bytes differ: got %d bytes, want %d", len(outcome.data), len(wantData))
			}
			decoded, _, err := image.Decode(bytes.NewReader(outcome.data))
			if err != nil {
				t.Fatal(err)
			}
			if decoded.Bounds().Dx() != testCase.wantSize || decoded.Bounds().Dy() != testCase.wantSize {
				t.Errorf("source dimensions = %dx%d, want %dx%d", decoded.Bounds().Dx(), decoded.Bounds().Dy(), testCase.wantSize, testCase.wantSize)
			}
		})
	}
}
