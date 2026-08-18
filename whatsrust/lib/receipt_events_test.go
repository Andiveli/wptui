package main

import (
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestReceiptEventConversionMapsReadReceiptsAndRejectsOthers(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	cases := []struct {
		name     string
		type_    types.ReceiptType
		wantOK   bool
		wantIDs  []string
		wantChat types.JID
	}{
		{name: "read", type_: types.ReceiptTypeRead, wantOK: true, wantIDs: []string{"first", "second"}, wantChat: chat},
		{name: "read self", type_: types.ReceiptTypeReadSelf, wantOK: true, wantIDs: []string{}, wantChat: chat},
		{name: "delivered is ignored", type_: types.ReceiptTypeDelivered},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			event := &events.Receipt{Type: tc.type_, MessageSource: types.MessageSource{Chat: chat}, MessageIDs: tc.wantIDs}
			got, ok := receiptEventFromEvent(event)
			if ok != tc.wantOK {
				t.Fatalf("ok = %v, want %v", ok, tc.wantOK)
			}
			if !ok {
				return
			}
			if got.chat != tc.wantChat || len(got.messageIDs) != len(tc.wantIDs) {
				t.Fatalf("receipt = %#v, want chat %v and %d IDs", got, tc.wantChat, len(tc.wantIDs))
			}
			for i := range tc.wantIDs {
				if got.messageIDs[i] != tc.wantIDs[i] {
					t.Fatalf("message ID %d = %q, want %q", i, got.messageIDs[i], tc.wantIDs[i])
				}
			}
		})
	}

	if _, ok := receiptEventFromEvent(nil); ok {
		t.Fatal("nil receipt must be ignored")
	}
}

func TestReceiptDispatchKeepsCArrayAndPayloadThroughCallback(t *testing.T) {
	source, err := os.ReadFile("receipt_events.go")
	if err != nil {
		t.Fatal(err)
	}
	body, ok := extractFunctionBody(string(source), "func dispatchReceiptEvent(receipt receiptEvent)")
	if !ok {
		t.Fatal("dispatchReceiptEvent function body not found in receipt_events.go")
	}
	for _, fragment := range []string{
		"C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(uintptr(0))))",
		"unsafe.Slice(cmessageIDs, n)",
		"C.CString(id)",
		"C.malloc(C.sizeof_ReceiptEvent)",
		"C.callReceiptEventCallback(eventHandler, &C.Event{",
	} {
		if !strings.Contains(body, fragment) {
			t.Fatalf("receipt dispatch must contain %q", fragment)
		}
	}
	callback := strings.Index(body, "C.callReceiptEventCallback(eventHandler, &C.Event{")
	arrayBuild := strings.Index(body, "unsafe.Slice(cmessageIDs, n)")
	if callback < arrayBuild {
		t.Fatal("receipt C array must be populated before the callback")
	}
}
