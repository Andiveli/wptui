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
	"sync"
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

func dispatchUndecryptableMessage(
	evt *events.UndecryptableMessage,
	dispatchViewOnceUnavailable func(types.MessageInfo, bool),
) {
	if !evt.IsUnavailable || evt.UnavailableType != events.UnavailableTypeViewOnce {
		return
	}
	dispatchViewOnceUnavailable(evt.Info, false)
}

type viewOnceUnavailableDispatcher struct {
	mu       sync.Mutex
	seen     map[string]struct{}
	dispatch func(types.MessageInfo, bool)
}

func newViewOnceUnavailableDispatcher(dispatch func(types.MessageInfo, bool)) *viewOnceUnavailableDispatcher {
	return &viewOnceUnavailableDispatcher{
		seen:     make(map[string]struct{}),
		dispatch: dispatch,
	}
}

func (dispatcher *viewOnceUnavailableDispatcher) dispatchOnce(info types.MessageInfo, isSync bool) {
	if info.ID != "" {
		dispatcher.mu.Lock()
		_, alreadySeen := dispatcher.seen[info.ID]
		if !alreadySeen {
			dispatcher.seen[info.ID] = struct{}{}
		}
		dispatcher.mu.Unlock()
		if alreadySeen {
			return
		}
	}
	dispatcher.dispatch(info, isSync)
}

func AddEventHandlers() {
	viewOnceDispatcher := newViewOnceUnavailableDispatcher(HandleViewOnceUnavailableMessage)
	client.AddEventHandler(func(rawEvt any) {
		messageActionCensusDiagnostic(rawEvt)
		switch evt := rawEvt.(type) {
		case *events.Connected:
			handleConnected(client.SendPresence, dispatchConnectedEvent, LOG_WARN)
		case *events.Presence:
			dispatchPresenceEvent(evt, client.Store.LIDs.GetPNForLID, rawPresenceProbe.record, rawPresenceProbe.update, dispatchPresenceCallback)
		case *events.MarkChatAsRead:
			dispatchMarkChatAsReadEvent(evt)
		case *events.AppStateSyncComplete:
			dispatchAppStateSyncComplete(evt)
		case *events.Message:
			// The message action router remains behind the canonical
			// dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage)
			// boundary; this handler only adds the view-once projection.
			dispatchIncomingMessage(evt, dispatchMessageActionEvent, HandleMessage, viewOnceDispatcher.dispatchOnce)
		case *events.UndecryptableMessage:
			dispatchUndecryptableMessage(evt, viewOnceDispatcher.dispatchOnce)
		case *events.Receipt:
			if receipt, ok := receiptEventFromEventWithClient(client, evt); ok {
				LOG_DEBUG("%#v was read by %s at %s", evt.MessageIDs, evt.SourceString(), evt.Timestamp)
				dispatchReceiptEvent(receipt)
			}
		case *events.HistorySync:
			dispatchHistorySync(evt, client.DangerousInternals().StoreHistoricalMessageSecrets, client.ParseWebMessage, func(parsed *events.Message) {
				dispatchIncomingMessage(parsed, dispatchMessageActionEvent, func(info types.MessageInfo, message *waE2E.Message, _ bool) {
					HandleMessage(info, message, true)
				}, viewOnceDispatcher.dispatchOnce)
			})
		}
	})
}
