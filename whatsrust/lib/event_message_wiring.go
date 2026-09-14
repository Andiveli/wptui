package main

import (
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func dispatchMessageFamily(evt *events.Message, dispatchViewOnce func(info types.MessageInfo, isSync bool)) {
	// The message action router remains behind the canonical
	// dispatchIncomingMessage boundary; this handler adds the view-once projection.
	dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage, dispatchViewOnce)
}
