package main

/*
#include <stdlib.h>
#include "callback_log_registration.h"

static void callPresenceWiringHandler(PresenceHandler hdl, JID from, bool unavailable, int64_t lastSeen) {
	hdl.callback(from, unavailable, lastSeen, hdl.user_data);
}
*/
import "C"

import (
	"unsafe"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func dispatchPresenceCallback(from string, unavailable bool, lastSeen int64) {
	cFrom := C.CString(from)
	defer C.free(unsafe.Pointer(cFrom))
	C.callPresenceWiringHandler(presenceHandler, cFrom, C.bool(unavailable), C.int64_t(lastSeen))
}

func AddEventHandlers() {
	client.AddEventHandler(func(rawEvt any) {
		messageActionCensusDiagnostic(rawEvt)
		switch evt := rawEvt.(type) {
		case *events.Connected:
			handleConnected(client.SendPresence, dispatchConnectedEvent, LOG_WARN)
		case *events.Presence:
			dispatchPresenceEvent(evt, client.Store.LIDs.GetPNForLID, rawPresenceProbe.record, rawPresenceProbe.update, dispatchPresenceCallback)
		case *events.MarkChatAsRead:
			LOG_DEBUG("MarkChatAsRead %v", evt.JID)
		case *events.AppStateSyncComplete:
			dispatchAppStateSyncComplete(evt)
		case *events.Message:
			dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage)
		case *events.Receipt:
			if receipt, ok := receiptEventFromEvent(evt); ok {
				LOG_DEBUG("%#v was read by %s at %s", evt.MessageIDs, evt.SourceString(), evt.Timestamp)
				dispatchReceiptEvent(receipt)
			}
		case *events.HistorySync:
			dispatchHistorySync(evt, client.DangerousInternals().StoreHistoricalMessageSecrets, client.ParseWebMessage, func(parsed *events.Message) {
				dispatchIncomingMessage(parsed, dispatchMessageActionEvent, func(info types.MessageInfo, message *waE2E.Message, _ bool) {
					HandleMessage(info, message, true)
				})
			})
		}
	})
}
