package main

import (
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func dispatchSyncCompleteFamily(evt *events.AppStateSyncComplete) {
	dispatchAppStateSyncComplete(evt)
}

func dispatchHistoryFamily(evt *events.HistorySync, client *whatsmeow.Client, dispatchViewOnce func(types.MessageInfo, bool)) {
	dispatchHistorySync(evt, client.DangerousInternals().StoreHistoricalMessageSecrets, client.ParseWebMessage, func(parsed *events.Message) {
		dispatchIncomingMessage(parsed, dispatchMessageActionEvent, func(info types.MessageInfo, message *waE2E.Message, _ bool) {
			HandleMessage(info, message, true)
		}, dispatchViewOnce)
	})
}

func dispatchReceiptFamily(evt *events.Receipt, client *whatsmeow.Client) {
	if receipt, ok := receiptEventFromEventWithClient(client, evt); ok {
		LOG_DEBUG("%#v was read by %s at %s", evt.MessageIDs, evt.SourceString(), evt.Timestamp)
		dispatchReceiptEvent(receipt)
	}
}
