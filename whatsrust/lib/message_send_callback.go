package main

import (
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

var normalMessageCallback = HandleMessage
var optimisticTextSentCallback = HandleOptimisticTextSent

func emitOutboundCallback(request textSendRequest, messageInfo types.MessageInfo, message *waE2E.Message) {
	if request.localSendID != 0 {
		optimisticTextSentCallback(request.localSendID, messageInfo, message)
		return
	}
	normalMessageCallback(messageInfo, message, false)
}
