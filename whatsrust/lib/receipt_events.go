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
	uint32_t size;
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

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

type receiptEvent struct {
	chat       types.JID
	messageIDs []string
	kind       uint8
}

type receiptChatCanonicalization string

const (
	receiptChatUnchanged       receiptChatCanonicalization = "unchanged"
	receiptChatLIDToPNResolved receiptChatCanonicalization = "lid_to_pn_resolved"
	receiptChatMappingMissing  receiptChatCanonicalization = "mapping_unavailable"
	receiptChatInvalid         receiptChatCanonicalization = "invalid"
)

func canonicalizeReceiptChat(c *whatsmeow.Client, chat, sender types.JID) (types.JID, receiptChatCanonicalization) {
	if chat.IsEmpty() {
		return types.JID{}, receiptChatInvalid
	}
	normalized := GetChatId(c, &chat, &sender)
	if normalized == "" {
		return types.JID{}, receiptChatInvalid
	}
	canonical, err := types.ParseJID(normalized)
	if err != nil || canonical.IsEmpty() {
		return types.JID{}, receiptChatInvalid
	}
	if chat.Server == types.HiddenUserServer && canonical.Server == types.HiddenUserServer {
		return canonical, receiptChatMappingMissing
	}
	if normalized != StrFromJid(chat) {
		return canonical, receiptChatLIDToPNResolved
	}
	return canonical, receiptChatUnchanged
}

func receiptEventFromEventWithClient(c *whatsmeow.Client, event *events.Receipt) (receiptEvent, bool) {
	if event == nil || (event.Type != types.ReceiptTypeRead && event.Type != types.ReceiptTypeReadSelf) {
		return receiptEvent{}, false
	}
	chat, outcome := canonicalizeReceiptChat(c, event.MessageSource.Chat, event.MessageSource.Sender)
	messageActionDiagnostic("classifier=receipt_canonicalization result=%s kind=receipt", outcome)
	if outcome == receiptChatInvalid {
		return receiptEvent{}, false
	}
	return receiptEvent{
		chat:       chat,
		messageIDs: event.MessageIDs,
		kind:       receiptKind(event.Type),
	}, true
}

func receiptEventFromEvent(event *events.Receipt) (receiptEvent, bool) {
	return receiptEventFromEventWithClient(lifecycleState.clientSnapshot(), event)
}

func receiptKind(kind types.ReceiptType) uint8 {
	if kind == types.ReceiptTypeReadSelf {
		return 1
	}
	return 0
}

func dispatchReceiptEvent(receipt receiptEvent) {
	n := len(receipt.messageIDs)
	var cmessageIDs **C.char
	if n > 0 {
		cmessageIDs = (**C.char)(C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(uintptr(0)))))
		if cmessageIDs == nil {
			return
		}
	}
	messageIDs := unsafe.Slice(cmessageIDs, n)
	for i, id := range receipt.messageIDs {
		messageIDs[i] = C.CString(id)
	}

	creceipt := (*C.ReceiptEvent)(C.malloc(C.sizeof_ReceiptEvent))
	if creceipt == nil {
		for _, id := range messageIDs {
			C.free(unsafe.Pointer(id))
		}
		C.free(unsafe.Pointer(cmessageIDs))
		return
	}
	creceipt.kind = C.uint8_t(receipt.kind)
	creceipt.id = jidToC(receipt.chat)
	creceipt.messageIDs = cmessageIDs
	creceipt.size = C.uint32_t(n)

	C.callReceiptEventCallback(eventHandler, &C.Event{
		kind: C.uint8_t(EventTypeReceipt),
		data: unsafe.Pointer(creceipt),
	})
	for _, id := range messageIDs {
		C.free(unsafe.Pointer(id))
	}
	C.free(unsafe.Pointer(cmessageIDs))
	C.free(unsafe.Pointer(creceipt.id))
	C.free(unsafe.Pointer(creceipt))
}
