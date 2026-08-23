package main

import (
	"context"
	"fmt"
	"mime"
	"os"
	"path/filepath"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"google.golang.org/protobuf/proto"
)

type uploadMediaFunc func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error)

// buildFileMessage maps one FFI file payload into its WhatsApp message without
// requiring a connected client. Keeping upload injectable lets the mapping stay
// deterministic and testable with fake upload responses.
func buildFileMessage(ctx context.Context, kind uint8, filePath string, caption *string, contextInfo *waE2E.ContextInfo, upload uploadMediaFunc) (*waE2E.Message, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("read file %s: %w", filePath, err)
	}
	mimetype := mime.TypeByExtension(filepath.Ext(filePath))

	uploadedMessage := func(mediaType whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
		uploaded, uploadErr := upload(ctx, data, mediaType)
		if uploadErr != nil {
			return whatsmeow.UploadResponse{}, fmt.Errorf("upload file: %w", uploadErr)
		}
		return uploaded, nil
	}

	switch kind {
	case FileTypeImage:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaImage)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{ImageMessage: &waE2E.ImageMessage{
			Caption:       caption,
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeVideo:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaVideo)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{VideoMessage: &waE2E.VideoMessage{
			Caption:       caption,
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeAudio:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaAudio)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{AudioMessage: &waE2E.AudioMessage{
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeDocument:
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaDocument)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{
			Caption:       caption,
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			FileName:      proto.String(filepath.Base(filePath)),
			ContextInfo:   contextInfo,
		}}, nil
	case FileTypeSticker:
		// WhatsApp stickers use the image media encryption keys.
		uploaded, uploadErr := uploadedMessage(whatsmeow.MediaImage)
		if uploadErr != nil {
			return nil, uploadErr
		}
		return &waE2E.Message{StickerMessage: &waE2E.StickerMessage{
			URL:           proto.String(uploaded.URL),
			DirectPath:    proto.String(uploaded.DirectPath),
			MediaKey:      uploaded.MediaKey,
			Mimetype:      proto.String(mimetype),
			FileEncSHA256: uploaded.FileEncSHA256,
			FileSHA256:    uploaded.FileSHA256,
			FileLength:    proto.Uint64(uint64(len(data))),
			ContextInfo:   contextInfo,
		}}, nil
	default:
		return nil, fmt.Errorf("unsupported file type: %v", kind)
	}
}
