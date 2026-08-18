package main

import (
	"google.golang.org/protobuf/proto"

	"go.mau.fi/whatsmeow/proto/waE2E"
)

const viewOnceUnavailablePlaceholder = "View-once media is unavailable here. View it on your phone."

func unavailableViewOnceMessage(message *waE2E.Message) (*waE2E.Message, bool) {
	if !containsViewOnceWrapper(message) {
		return message, false
	}
	return &waE2E.Message{Conversation: proto.String(viewOnceUnavailablePlaceholder)}, true
}

func containsViewOnceWrapper(message *waE2E.Message) bool {
	for message != nil {
		switch {
		case message.GetViewOnceMessage() != nil || message.GetViewOnceMessageV2() != nil || message.GetViewOnceMessageV2Extension() != nil:
			return true
		case message.GetDeviceSentMessage().GetMessage() != nil:
			message = message.GetDeviceSentMessage().GetMessage()
		case message.GetBotInvokeMessage().GetMessage() != nil:
			message = message.GetBotInvokeMessage().GetMessage()
		case message.GetEphemeralMessage().GetMessage() != nil:
			message = message.GetEphemeralMessage().GetMessage()
		case message.GetLottieStickerMessage().GetMessage() != nil:
			message = message.GetLottieStickerMessage().GetMessage()
		case message.GetDocumentWithCaptionMessage().GetMessage() != nil:
			message = message.GetDocumentWithCaptionMessage().GetMessage()
		case message.GetEditedMessage().GetMessage() != nil:
			message = message.GetEditedMessage().GetMessage()
		default:
			return false
		}
	}
	return false
}
