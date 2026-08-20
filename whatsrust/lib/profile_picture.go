package main

/*
#include <stdint.h>
#include <stdlib.h>
#include <stdbool.h>

typedef struct {
	uint8_t status;
	char* picture_id;
	char* picture_type;
	uint8_t* data;
	uint32_t size;
} ProfilePictureResult;
*/
import "C"

import (
	"bytes"
	"context"
	"errors"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"strings"
	"time"
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

const (
	profilePictureStatusAvailable uint8 = iota
	profilePictureStatusUnavailable
	profilePictureStatusInvalidJID
	profilePictureStatusClientUnavailable
	profilePictureStatusCancelled
	profilePictureStatusMetadataFailed
	profilePictureStatusEmptyURL
	profilePictureStatusDownloadFailed
	profilePictureStatusOversized
	profilePictureStatusInvalidImage
	profilePictureStatusUnauthorized
	profilePictureStatusNotSet
)

const (
	profilePictureMaxSize int64 = 512 * 1024
	profilePictureTimeout       = 8 * time.Second
)

var errProfilePictureOversized = errors.New("profile picture exceeds size limit")

type profilePictureLookup func(context.Context, types.JID, *whatsmeow.GetProfilePictureParams) (*types.ProfilePictureInfo, error)
type profilePictureDownload func(context.Context, string, int64) ([]byte, error)

type profilePictureTarget struct {
	IsCommunity bool
	CommonGID   types.JID
}

type profilePictureOutcome struct {
	status      uint8
	pictureID   string
	pictureType string
	data        []byte
}

func fetchProfilePicture(ctx context.Context, jidText string, target profilePictureTarget, lookup profilePictureLookup, download profilePictureDownload) profilePictureOutcome {
	jidText = strings.TrimSpace(jidText)
	jid, err := types.ParseJID(jidText)
	if err != nil || strings.Count(jidText, "@") != 1 || jid.User == "" || (jid.Server != types.DefaultUserServer && jid.Server != types.HiddenUserServer && jid.Server != types.GroupServer) {
		return profilePictureOutcome{status: profilePictureStatusInvalidJID}
	}
	if lookup == nil || download == nil {
		return profilePictureOutcome{status: profilePictureStatusClientUnavailable}
	}

	info, err := lookup(ctx, jid.ToNonAD(), &whatsmeow.GetProfilePictureParams{
		Preview:     true,
		IsCommunity: target.IsCommunity,
		CommonGID:   target.CommonGID,
	})
	if errors.Is(err, whatsmeow.ErrProfilePictureUnauthorized) {
		return profilePictureOutcome{status: profilePictureStatusUnauthorized}
	}
	if errors.Is(err, whatsmeow.ErrProfilePictureNotSet) {
		return profilePictureOutcome{status: profilePictureStatusNotSet}
	}
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) || ctx.Err() != nil {
		return profilePictureOutcome{status: profilePictureStatusCancelled}
	}
	if err != nil || info == nil {
		return profilePictureOutcome{status: profilePictureStatusMetadataFailed}
	}
	if info.URL == "" {
		return profilePictureOutcome{status: profilePictureStatusEmptyURL, pictureID: info.ID, pictureType: info.Type}
	}

	data, err := download(ctx, info.URL, profilePictureMaxSize)
	if errors.Is(err, context.Canceled) || errors.Is(err, context.DeadlineExceeded) || ctx.Err() != nil {
		return profilePictureOutcome{status: profilePictureStatusCancelled, pictureID: info.ID, pictureType: info.Type}
	}
	if errors.Is(err, errProfilePictureOversized) {
		return profilePictureOutcome{status: profilePictureStatusOversized, pictureID: info.ID, pictureType: info.Type}
	}
	if err != nil {
		return profilePictureOutcome{status: profilePictureStatusDownloadFailed, pictureID: info.ID, pictureType: info.Type}
	}
	if int64(len(data)) > profilePictureMaxSize {
		return profilePictureOutcome{status: profilePictureStatusOversized, pictureID: info.ID, pictureType: info.Type}
	}
	if len(data) == 0 {
		return profilePictureOutcome{status: profilePictureStatusInvalidImage, pictureID: info.ID, pictureType: info.Type}
	}
	if _, _, err := image.DecodeConfig(bytes.NewReader(data)); err != nil {
		return profilePictureOutcome{status: profilePictureStatusInvalidImage, pictureID: info.ID, pictureType: info.Type}
	}
	return profilePictureOutcome{status: profilePictureStatusAvailable, pictureID: info.ID, pictureType: info.Type, data: data}
}

func downloadProfilePicture(ctx context.Context, url string, limit int64) ([]byte, error) {
	response, err := client.DangerousInternals().DoMediaDownloadRequest(ctx, url)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, limit+1))
	if err != nil {
		return nil, err
	}
	if int64(len(data)) > limit {
		return nil, errProfilePictureOversized
	}
	return data, nil
}

func profilePictureToC(outcome profilePictureOutcome) C.ProfilePictureResult {
	result := C.ProfilePictureResult{status: C.uint8_t(outcome.status)}
	if outcome.pictureID != "" {
		result.picture_id = C.CString(outcome.pictureID)
	}
	if outcome.pictureType != "" {
		result.picture_type = C.CString(outcome.pictureType)
	}
	if len(outcome.data) > 0 {
		result.data = (*C.uint8_t)(C.CBytes(outcome.data))
		result.size = C.uint32_t(len(outcome.data))
	}
	return result
}

//export C_GetProfilePicture
func C_GetProfilePicture(jid *C.char, isCommunity C.bool, commonGID *C.char) C.ProfilePictureResult {
	if jid == nil {
		return C.ProfilePictureResult{status: C.uint8_t(profilePictureStatusInvalidJID)}
	}
	if client == nil {
		return C.ProfilePictureResult{status: C.uint8_t(profilePictureStatusClientUnavailable)}
	}
	ctx, cancel := context.WithTimeout(context.Background(), profilePictureTimeout)
	defer cancel()
	target := profilePictureTarget{IsCommunity: bool(isCommunity)}
	if commonGID != nil && C.GoString(commonGID) != "" {
		if parsed, err := types.ParseJID(C.GoString(commonGID)); err == nil {
			target.CommonGID = parsed.ToNonAD()
		}
	}
	return profilePictureToC(fetchProfilePicture(ctx, C.GoString(jid), target, client.GetProfilePictureInfo, downloadProfilePicture))
}

//export C_FreeProfilePicture
func C_FreeProfilePicture(result C.ProfilePictureResult) {
	C.free(unsafe.Pointer(result.picture_id))
	C.free(unsafe.Pointer(result.picture_type))
	C.free(unsafe.Pointer(result.data))
}
