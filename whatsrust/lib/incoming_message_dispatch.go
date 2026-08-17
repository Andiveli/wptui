package main

import (
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

// dispatchIncomingMessage keeps action envelopes out of the ordinary message
// path, whose insertion logic would otherwise treat an edit body as a base message.
func dispatchIncomingMessage(
	evt *events.Message,
	dispatchAction func(messageActionEvent),
	dispatchMessage func(types.MessageInfo, *waE2E.Message, bool),
) {
	dispatchIncomingMessageWithDecrypt(evt, dispatchAction, dispatchMessage, decryptSecretEncryptedMessage)
}

func dispatchIncomingMessageWithDecrypt(
	evt *events.Message,
	dispatchAction func(messageActionEvent),
	dispatchMessage func(types.MessageInfo, *waE2E.Message, bool),
	decrypt decryptSecretEncryptedMessageFunc,
) {
	if action, ok := messageActionEventFromSecretEncryptedMessage(evt, decrypt); ok {
		dispatchAction(action)
		return
	}
	if action, ok := messageActionEventFromIncomingMessage(evt); ok {
		dispatchAction(action)
		return
	}
	rawMessage := evt.RawMessage
	if rawMessage == nil && evt.SourceWebMsg != nil {
		rawMessage = evt.SourceWebMsg.GetMessage()
	}
	if message, ok := unavailableViewOnceMessage(rawMessage); ok {
		dispatchMessage(evt.Info, message, false)
		return
	}
	dispatchMessage(evt.Info, evt.Message, false)
}
