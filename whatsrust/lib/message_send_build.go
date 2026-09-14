package main

import (
	"context"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

type textSendQuote struct {
	stanzaID    string
	participant string
	remoteJID   string
	content     *waE2E.Message
}

type textSendRequest struct {
	messageType   uint8
	chat          types.JID
	text          string
	mentionedJIDs []string
	fileKind      uint8
	filePath      string
	caption       *string
	quote         *textSendQuote
	localSendID   uint64
}

func contentToWaE2EMessage(messageType uint8, text string, mentioned []string, fileKind uint8, filePath string, caption *string, contextInfo *waE2E.ContextInfo, upload uploadMediaFunc) *waE2E.Message {
	setMentionedJIDsFromStrings(contextInfo, mentioned)
	if messageType == MessageTypeText {
		return &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: &text, ContextInfo: contextInfo}}
	}
	message, err := buildFileMessage(context.Background(), fileKind, filePath, caption, contextInfo, upload)
	if err != nil {
		panic(err)
	}
	return message
}

func setMentionedJIDsFromStrings(contextInfo *waE2E.ContextInfo, mentioned []string) {
	if contextInfo != nil {
		contextInfo.MentionedJID = mentioned
	}
}

func normalizeMentionedJIDs(mentioned []string) []string {
	result := make([]string, 0, len(mentioned))
	for _, value := range mentioned {
		parsed, err := types.ParseJID(value)
		if err == nil && !parsed.IsEmpty() {
			result = append(result, parsed.ToNonAD().String())
		}
	}
	return result
}

func quotedContextInfo(id, sender, chat string) *waE2E.ContextInfo {
	return &waE2E.ContextInfo{StanzaID: &id, Participant: &sender, RemoteJID: &chat}
}

func quotedTextMessage(text string) *waE2E.Message {
	return &waE2E.Message{Conversation: &text}
}

func quotedFileMessage(kind uint8, caption string) *waE2E.Message {
	var captionPtr *string
	if caption != "" {
		captionPtr = &caption
	}
	switch kind {
	case FileTypeImage:
		return &waE2E.Message{ImageMessage: &waE2E.ImageMessage{Caption: captionPtr}}
	case FileTypeVideo:
		return &waE2E.Message{VideoMessage: &waE2E.VideoMessage{Caption: captionPtr}}
	case FileTypeAudio:
		return &waE2E.Message{AudioMessage: &waE2E.AudioMessage{}}
	case FileTypeDocument:
		return &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{Caption: captionPtr}}
	case FileTypeSticker:
		return &waE2E.Message{StickerMessage: &waE2E.StickerMessage{}}
	default:
		return nil
	}
}

func buildOutboundMessage(ctx context.Context, request textSendRequest, contextInfo *waE2E.ContextInfo, upload uploadMediaFunc) (*waE2E.Message, error) {
	setMentionedJIDsFromStrings(contextInfo, normalizeMentionedJIDs(request.mentionedJIDs))
	if request.messageType == MessageTypeText {
		return &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: &request.text, ContextInfo: contextInfo}}, nil
	}
	return buildFileMessage(ctx, request.fileKind, request.filePath, request.caption, contextInfo, upload)
}
