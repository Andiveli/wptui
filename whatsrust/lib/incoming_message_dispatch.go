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
	dispatchViewOnceUnavailable ...func(types.MessageInfo, bool),
) {
	viewOnceDispatcher := HandleViewOnceUnavailableMessage
	if len(dispatchViewOnceUnavailable) > 0 && dispatchViewOnceUnavailable[0] != nil {
		viewOnceDispatcher = dispatchViewOnceUnavailable[0]
	}
	dispatchIncomingMessageWithDecrypt(evt, dispatchAction, dispatchMessage, decryptSecretEncryptedMessage, viewOnceDispatcher)
}

func dispatchIncomingMessageWithDecrypt(
	evt *events.Message,
	dispatchAction func(messageActionEvent),
	dispatchMessage func(types.MessageInfo, *waE2E.Message, bool),
	decrypt decryptSecretEncryptedMessageFunc,
	dispatchViewOnceUnavailable ...func(types.MessageInfo, bool),
) {
	viewOnceDispatcher := HandleViewOnceUnavailableMessage
	if len(dispatchViewOnceUnavailable) > 0 && dispatchViewOnceUnavailable[0] != nil {
		viewOnceDispatcher = dispatchViewOnceUnavailable[0]
	}
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
	if rawMessage == nil {
		rawMessage = evt.Message
	}
	if isUnavailableViewOnceMessage(rawMessage) {
		viewOnceDispatcher(evt.Info, false)
		return
	}
	dispatchMessage(evt.Info, evt.Message, false)
}
