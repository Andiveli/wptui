package main

/*
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef const char* JID;

typedef struct {
	uint8_t kind;
	JID id;
	char* const* messageIDs;
	size_t size;
} ReceiptEvent;

typedef struct {
	uint8_t kind;
	void* data;
} Event;

typedef void (*EventCallback)(const Event*, void*);
typedef struct {
	EventCallback callback;
	void* user_data;
} EventHandler;

static void callReceiptEventCallback(EventHandler hdl, const Event* event) {
	hdl.callback(event, hdl.user_data);
}
*/
import "C"

import (
	"unsafe"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

type receiptEvent struct {
	chat       types.JID
	messageIDs []string
}

func receiptEventFromEvent(event *events.Receipt) (receiptEvent, bool) {
	if event == nil || (event.Type != types.ReceiptTypeRead && event.Type != types.ReceiptTypeReadSelf) {
		return receiptEvent{}, false
	}
	return receiptEvent{
		chat:       event.MessageSource.Chat,
		messageIDs: event.MessageIDs,
	}, true
}

func dispatchReceiptEvent(receipt receiptEvent) {
	n := len(receipt.messageIDs)
	cmessageIDs := (**C.char)(C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(uintptr(0)))))
	messageIDs := unsafe.Slice(cmessageIDs, n)
	for i, id := range receipt.messageIDs {
		messageIDs[i] = C.CString(id)
	}

	creceipt := (*C.ReceiptEvent)(C.malloc(C.sizeof_ReceiptEvent))
	creceipt.kind = C.uint8_t(EventTypeReceipt)
	creceipt.id = jidToC(receipt.chat)
	creceipt.messageIDs = cmessageIDs
	creceipt.size = C.size_t(n)

	C.callReceiptEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeReceipt),
		data: unsafe.Pointer(creceipt),
	})
}
