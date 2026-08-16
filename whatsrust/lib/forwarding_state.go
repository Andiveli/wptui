package main

import waE2E "go.mau.fi/whatsmeow/proto/waE2E"

type forwardingState struct {
	isForwarded bool
	score       uint32
}

func forwardingStateFromContext(context *waE2E.ContextInfo) forwardingState {
	if context == nil {
		return forwardingState{}
	}
	return forwardingState{isForwarded: context.GetIsForwarded(), score: context.GetForwardingScore()}
}

func forwardingStateFromMessage(message *waE2E.Message) forwardingState {
	switch {
	case message == nil:
		return forwardingState{}
	case message.GetExtendedTextMessage() != nil:
		return forwardingStateFromContext(message.GetExtendedTextMessage().GetContextInfo())
	case message.GetImageMessage() != nil:
		return forwardingStateFromContext(message.GetImageMessage().GetContextInfo())
	case message.GetVideoMessage() != nil:
		return forwardingStateFromContext(message.GetVideoMessage().GetContextInfo())
	case message.GetAudioMessage() != nil:
		return forwardingStateFromContext(message.GetAudioMessage().GetContextInfo())
	case message.GetDocumentMessage() != nil:
		return forwardingStateFromContext(message.GetDocumentMessage().GetContextInfo())
	case message.GetStickerMessage() != nil:
		return forwardingStateFromContext(message.GetStickerMessage().GetContextInfo())
	default:
		return forwardingState{}
	}
}
