package main

import (
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

// dispatchEventFamily keeps registration orchestration separate from the
// cohesive handlers for each event family.
func dispatchEventFamily(rawEvt any, client *whatsmeow.Client, dispatchViewOnce func(types.MessageInfo, bool)) {
	switch evt := rawEvt.(type) {
	case *events.Connected:
		dispatchConnectionFamily(client)
	case *events.Presence:
		dispatchPresenceFamily(evt, client)
	case *events.MarkChatAsRead:
		dispatchReadFamily(evt)
	case *events.AppStateSyncComplete:
		dispatchSyncCompleteFamily(evt)
	case *events.Message:
		dispatchMessageFamily(evt, dispatchViewOnce)
	case *events.UndecryptableMessage:
		dispatchUndecryptableMessage(evt, dispatchViewOnce)
	case *events.Receipt:
		dispatchReceiptFamily(evt, client)
	case *events.HistorySync:
		dispatchHistoryFamily(evt, client, dispatchViewOnce)
	}
}
