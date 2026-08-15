package main

import (
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waHistorySync"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestAppStateSyncCompleteOnlyAcceptsRegularPatch(t *testing.T) {
	cases := []struct {
		caseName  string
		patchName appstate.WAPatchName
		want      bool
	}{
		{caseName: "regular", patchName: appstate.WAPatchRegular, want: true},
		{caseName: "critical", patchName: appstate.WAPatchCriticalBlock, want: false},
	}
	for _, tc := range cases {
		t.Run(tc.caseName, func(t *testing.T) {
			if got := appStateSyncComplete(&events.AppStateSyncComplete{Name: tc.patchName}); got != tc.want {
				t.Fatalf("appStateSyncComplete() = %v, want %v", got, tc.want)
			}
		})
	}
	if appStateSyncComplete(nil) {
		t.Fatal("nil app-state event must be ignored")
	}
}

func TestChatSyncEventPreservesIdentityAndTimestamp(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	cases := []struct {
		name string
		id   string
		ts   uint64
		want int64
		ok   bool
	}{
		{name: "timestamp", id: chat.String(), ts: 42, want: 42, ok: true},
		{name: "empty timestamp", id: chat.String(), want: 0, ok: true},
		{name: "invalid chat", id: "1.2.3@server"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, ok := chatSyncEventFromConversation(&waHistorySync.Conversation{ID: &tc.id, LastMsgTimestamp: &tc.ts})
			if ok != tc.ok {
				t.Fatalf("ok = %v, want %v", ok, tc.ok)
			}
			if ok && (got.chat != chat || got.lastMessageTime != tc.want) {
				t.Fatalf("chat event = %#v, want %v at %d", got, chat, tc.want)
			}
		})
	}
}

func TestCountSyncMessagesIncludesEmptyAndNilConversations(t *testing.T) {
	if got := countSyncMessages([]*waHistorySync.Conversation{
		{Messages: []*waHistorySync.HistorySyncMsg{{}, {}}},
		{},
		nil,
	}); got != 2 {
		t.Fatalf("countSyncMessages() = %d, want 2", got)
	}
}

func TestHistorySyncDispatchKeepsProgressChatAndMessageOrdering(t *testing.T) {
	source, err := os.ReadFile("sync_events.go")
	if err != nil {
		t.Fatal(err)
	}
	body, ok := extractFunctionBody(string(source), "func dispatchHistorySync(")
	if !ok {
		t.Fatal("dispatchHistorySync function body not found")
	}
	for _, fragment := range []string{
		"C.malloc(C.size_t(unsafe.Sizeof(uint8(0)))",
		"EventTypeSyncProgress",
		"C.free(unsafe.Pointer(cpercent))",
		"EventTypeChat",
		"conversation.GetMessages()",
		"dispatchMessage(parsed)",
	} {
		if !strings.Contains(body, fragment) {
			t.Fatalf("history sync dispatch must contain %q", fragment)
		}
	}
	if strings.Index(body, "EventTypeSyncProgress") > strings.Index(body, "EventTypeChat") {
		t.Fatal("sync progress must be emitted before chat lifecycle events")
	}
}
