package main

import (
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
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
		wantKind uint8
	}{
		{name: "read", type_: types.ReceiptTypeRead, wantOK: true, wantIDs: []string{"first", "second"}, wantChat: chat, wantKind: 0},
		{name: "read self", type_: types.ReceiptTypeReadSelf, wantOK: true, wantIDs: []string{}, wantChat: chat, wantKind: 1},
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
			if got.chat != tc.wantChat || got.kind != tc.wantKind || len(got.messageIDs) != len(tc.wantIDs) {
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

func receiptTestClient(pn types.JID) *whatsmeow.Client {
	return &whatsmeow.Client{Store: &store.Device{LIDs: chatIdentityLIDStore{pn: pn}}}
}

func TestReceiptChatCanonicalizationMatchesMessageIdentity(t *testing.T) {
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	lid := types.NewJID("alice", types.HiddenUserServer)
	previous := client
	client = receiptTestClient(pn)
	defer func() { client = previous }()

	messageInfo := normalizeMessageInfo(types.MessageInfo{MessageSource: types.MessageSource{Chat: lid, Sender: lid}})
	receipt, ok := receiptEventFromEventWithClient(client, &events.Receipt{
		Type:          types.ReceiptTypeRead,
		MessageSource: types.MessageSource{Chat: lid, Sender: lid},
	})
	if !ok {
		t.Fatal("LID receipt must be accepted when its PN mapping is available")
	}
	if messageInfo.Chat != receipt.chat {
		t.Fatalf("message chat = %v, receipt chat = %v", messageInfo.Chat, receipt.chat)
	}
}

func TestReceiptChatCanonicalizationPreservesGroupsStatusAndUnavailableLIDs(t *testing.T) {
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	client := receiptTestClient(pn)
	cases := []struct {
		name    string
		chat    types.JID
		outcome receiptChatCanonicalization
		want    string
	}{
		{name: "group", chat: types.NewJID("12345-678", types.GroupServer), outcome: receiptChatUnchanged, want: "12345-678@g.us"},
		{name: "status", chat: types.NewJID("status", types.BroadcastServer), outcome: receiptChatUnchanged, want: "status@broadcast"},
		{name: "unavailable LID mapping", chat: types.NewJID("alice", types.HiddenUserServer), outcome: receiptChatMappingMissing, want: "alice@lid"},
	}
	client.Store.LIDs = chatIdentityLIDStore{pn: types.JID{}}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if tc.name == "group" || tc.name == "status" {
				client.Store.LIDs = chatIdentityLIDStore{pn: pn}
			}
			got, outcome := canonicalizeReceiptChat(client, tc.chat, types.JID{})
			if outcome != tc.outcome || got.String() != tc.want {
				t.Fatalf("canonicalizeReceiptChat() = %v, %q; want %v, %q", got, outcome, tc.outcome, tc.want)
			}
			if tc.name == "group" || tc.name == "status" {
				client.Store.LIDs = chatIdentityLIDStore{pn: types.JID{}}
			}
		})
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

func TestReceiptDispatchAndRustDecoderSupportEmptyMessageIDs(t *testing.T) {
	goSource, err := os.ReadFile("receipt_events.go")
	if err != nil {
		t.Fatal(err)
	}
	goBody, ok := extractFunctionBody(string(goSource), "func dispatchReceiptEvent(receipt receiptEvent)")
	if !ok {
		t.Fatal("dispatchReceiptEvent function body not found in receipt_events.go")
	}
	for _, fragment := range []string{
		"var cmessageIDs **C.char",
		"if n > 0 {",
		"creceipt.messageIDs = cmessageIDs",
		"creceipt.size = C.uint32_t(n)",
	} {
		if !strings.Contains(goBody, fragment) {
			t.Fatalf("empty receipt dispatch must contain %q", fragment)
		}
	}

	rustSource, err := os.ReadFile("../src/events.rs")
	if err != nil {
		t.Fatal(err)
	}
	rustText := string(rustSource)
	if !strings.Contains(rustText, "if receipt.count == 0 { &[] } else {") {
		t.Fatal("Rust receipt decoder must avoid dereferencing the message ID pointer for empty arrays")
	}
}
