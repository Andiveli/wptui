package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"testing"
	"time"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

// The trace wrapper must retain markReadResult's legacy codes and single call.
// Unknown SendAppState errors deliberately remain "other": their text may
// contain identities, keys, payloads, or describe a failure after the send.
func TestReadSyncTracePreservesBridgeResultAndNeverExportsErrorText(t *testing.T) {
	for _, tt := range []struct {
		name, category string
		err            error
		unavailable    bool
		status         int
	}{
		{name: "ok", category: "none", status: 0},
		{name: "unavailable", category: "unavailable", unavailable: true, status: 1},
		{name: "cancelled", category: "cancelled", err: fmt.Errorf("private: %w", context.Canceled), status: 2},
		{name: "timeout", category: "timeout", err: fmt.Errorf("private: %w", context.DeadlineExceeded), status: 2},
		{name: "unknown", category: "other", err: errors.New("private-chat@s.whatsapp.net private-message-id 1700000042 private message text key=secret payload=secret"), status: 2},
	} {
		t.Run(tt.name, func(t *testing.T) {
			for _, enabled := range []bool{false, true} {
				var report bytes.Buffer
				var output io.Writer
				if enabled {
					output = &report
				}
				calls := 0
				var send func() error
				if !tt.unavailable {
					send = func() error { calls++; return tt.err }
				}
				got := markReadResultWithTrace(send, output)
				wantCalls := 1
				if tt.unavailable {
					wantCalls = 0
				}
				want := ""
				if enabled {
					want = fmt.Sprintf("stage=bridge status=%d category=%s\n", tt.status, tt.category)
				}
				if int(got) != tt.status || calls != wantCalls || report.String() != want {
					t.Fatalf("status=%d calls=%d report=%q; want status=%d calls=%d report=%q", got, calls, report.String(), tt.status, wantCalls, want)
				}
			}
		})
	}
}

func TestChatReadSyncBuildsOnePatchWithLatestIdentity(t *testing.T) {
	tests := []struct {
		name        string
		chat        string
		fromMe      bool
		participant string
		wantPart    string
	}{
		{name: "dm", chat: "alice@s.whatsapp.net", wantPart: ""},
		{name: "group", chat: "group@g.us", fromMe: true, participant: "alice@s.whatsapp.net", wantPart: "alice@s.whatsapp.net"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var calls int
			var got appstate.PatchInfo
			err := sendChatReadSync(context.Background(), tt.chat, "latest", 42, tt.fromMe, tt.participant,
				func(_ context.Context, patch appstate.PatchInfo) error {
					calls++
					got = patch
					return nil
				})
			if err != nil {
				t.Fatal(err)
			}
			if calls != 1 {
				t.Fatalf("send calls = %d, want 1", calls)
			}
			if len(got.Mutations) != 1 || got.Mutations[0].Index[1] != tt.chat {
				t.Fatalf("patch target = %#v, want %s", got.Mutations, tt.chat)
			}
			action := got.Mutations[0].Value.GetMarkChatAsReadAction()
			if action == nil || !action.GetRead() || action.GetMessageRange().GetLastMessageTimestamp() != 42 {
				t.Fatalf("action = %#v, want read at 42", action)
			}
			message := action.GetMessageRange().GetMessages()[0]
			if message.GetKey().GetRemoteJID() != tt.chat || message.GetKey().GetID() != "latest" || message.GetKey().GetFromMe() != tt.fromMe {
				t.Fatalf("message key = %#v, want chat/id/from-me truth", message.GetKey())
			}
			if message.GetKey().GetParticipant() != tt.wantPart {
				t.Fatalf("participant = %q, want %q", message.GetKey().GetParticipant(), tt.wantPart)
			}
		})
	}
}

func TestMarkChatAsReadEventCopiesRangeIdentity(t *testing.T) {
	chat, err := types.ParseJID("alice@s.whatsapp.net")
	if err != nil {
		t.Fatal(err)
	}
	id := "latest"
	fromMe := false
	event, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{
		JID: chat,
		Action: &waSyncAction.MarkChatAsReadAction{
			Read: proto.Bool(true),
			MessageRange: &waSyncAction.SyncActionMessageRange{
				LastMessageTimestamp: proto.Int64(42),
				Messages: []*waSyncAction.SyncActionMessage{{
					Key: &waCommon.MessageKey{RemoteJID: proto.String(chat.String()), ID: &id, FromMe: &fromMe},
				}},
			},
		},
	})
	if !ok || event.chat != chat.String() || event.messageID != id || event.timestamp != 42 || !event.read {
		t.Fatalf("event = %#v, ok = %v", event, ok)
	}
}

// Pin the dependency contract: unread is app-state, not a message receipt,
// and a chat without messages does not require an invented message identity.
func TestPinnedMarkChatAsReadSupportsBothStatesAndEmptyChats(t *testing.T) {
	chat, err := types.ParseJID("group@g.us")
	if err != nil {
		t.Fatal(err)
	}
	for _, read := range []bool{false, true} {
		for _, empty := range []bool{false, true} {
			var key *waCommon.MessageKey
			if !empty {
				key = &waCommon.MessageKey{
					RemoteJID: proto.String(chat.String()), ID: proto.String("latest"),
					FromMe: proto.Bool(true), Participant: proto.String("alice@s.whatsapp.net"),
				}
			}
			patch := appstate.BuildMarkChatAsRead(chat, read, time.Unix(42, 0), key)
			if patch.Type != appstate.WAPatchRegularLow || len(patch.Mutations) != 1 {
				t.Fatalf("patch = %#v, want one regular-low mutation", patch)
			}
			mutation := patch.Mutations[0]
			if len(mutation.Index) != 2 || mutation.Index[0] != appstate.IndexMarkChatAsRead || mutation.Index[1] != chat.String() {
				t.Fatalf("index = %#v, want chat-level read action", mutation.Index)
			}
			action := mutation.Value.GetMarkChatAsReadAction()
			if action == nil || action.GetRead() != read || action.GetMessageRange().GetLastMessageTimestamp() != 42 {
				t.Fatalf("read=%v empty=%v: action = %#v", read, empty, action)
			}
			messages := action.GetMessageRange().GetMessages()
			if empty {
				if len(messages) != 0 {
					t.Fatalf("empty chat gained message identities: %#v", messages)
				}
			} else if len(messages) != 1 || !proto.Equal(messages[0].GetKey(), key) {
				t.Fatalf("message/group identity changed: %#v", messages)
			}
		}
	}
}

func TestMarkChatAsReadEventPreservesUnreadGroupIdentity(t *testing.T) {
	chat, err := types.ParseJID("group@g.us")
	if err != nil {
		t.Fatal(err)
	}
	for _, read := range []bool{false, true} {
		patch := appstate.BuildMarkChatAsRead(chat, read, time.Unix(42, 0), &waCommon.MessageKey{
			RemoteJID: proto.String(chat.String()), ID: proto.String("latest"),
			FromMe: proto.Bool(true), Participant: proto.String("alice@s.whatsapp.net"),
		})
		event, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{
			JID: chat, Action: patch.Mutations[0].Value.GetMarkChatAsReadAction(),
		})
		if !ok || event.chat != chat.String() || event.read != read || event.messageID != "latest" || event.timestamp != 42 || !event.fromMe || event.participant != "alice@s.whatsapp.net" {
			t.Fatalf("read=%v: event = %#v, ok = %v", read, event, ok)
		}
	}
}

func TestMarkChatAsReadEventAcceptsUnreadWithoutRange(t *testing.T) {
	chat, err := types.ParseJID("empty@s.whatsapp.net")
	if err != nil {
		t.Fatal(err)
	}
	event, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{
		JID: chat, Action: &waSyncAction.MarkChatAsReadAction{Read: proto.Bool(false)},
	})
	if !ok || event.chat != chat.String() || event.read || event.messageID != "" || event.timestamp != 0 || event.fromMe || event.participant != "" {
		t.Fatalf("event = %#v, ok = %v, want unread without fabricated identity", event, ok)
	}
}

func TestEmptyChatReadStatePatchSurvivesIncomingAdapter(t *testing.T) {
	chat, err := types.ParseJID("empty@s.whatsapp.net")
	if err != nil {
		t.Fatal(err)
	}
	for _, read := range []bool{false, true} {
		t.Run(fmt.Sprintf("read=%v", read), func(t *testing.T) {
			patch, err := buildChatReadStateSyncPatch("empty@s.whatsapp.net", "", read, 0, false, "")
			if err != nil {
				t.Fatal(err)
			}
			if len(patch.Mutations) != 1 {
				t.Fatalf("mutations = %d, want 1", len(patch.Mutations))
			}
			event, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{
				JID: chat, Action: patch.Mutations[0].Value.GetMarkChatAsReadAction(),
			})
			if !ok || event.chat != chat.String() || event.read != read || event.messageID != "" || event.timestamp != 0 || event.fromMe || event.participant != "" {
				t.Fatalf("event = %#v, ok = %v, want read=%v without fabricated identity", event, ok, read)
			}
		})
	}
}

func TestMarkChatAsReadEventRejectsMissingRange(t *testing.T) {
	if _, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{}); ok {
		t.Fatal("missing range was accepted")
	}
}

func TestChatReadStateSyncSendsBothStatesWithOptionalIdentity(t *testing.T) {
	for _, read := range []bool{false, true} {
		for _, tt := range []struct {
			name, chat, message, participant string
			fromMe                           bool
		}{
			{name: "dm", chat: "alice@s.whatsapp.net", message: "latest"},
			{name: "group", chat: "group@g.us", message: "latest", participant: "alice@s.whatsapp.net", fromMe: true},
			{name: "empty", chat: "alice@s.whatsapp.net"},
		} {
			t.Run(fmt.Sprintf("%s/read=%v", tt.name, read), func(t *testing.T) {
				calls := 0
				err := sendChatReadStateSync(context.Background(), tt.chat, tt.message, read, 42, tt.fromMe, tt.participant,
					func(_ context.Context, patch appstate.PatchInfo) error {
						calls++
						if patch.Type != appstate.WAPatchRegularLow || len(patch.Mutations) != 1 {
							t.Fatalf("patch = %#v, want one app-state mutation", patch)
						}
						mutation := patch.Mutations[0]
						if len(mutation.Index) != 2 || mutation.Index[0] != appstate.IndexMarkChatAsRead || mutation.Index[1] != tt.chat {
							t.Fatalf("index = %#v", mutation.Index)
						}
						action := mutation.Value.GetMarkChatAsReadAction()
						if action == nil || action.GetRead() != read || action.GetMessageRange().GetLastMessageTimestamp() != 42 {
							t.Fatalf("action = %#v, want read=%v at 42", action, read)
						}
						messages := action.GetMessageRange().GetMessages()
						if tt.message == "" {
							if len(messages) != 0 {
								t.Fatalf("empty chat has fabricated identity: %#v", messages)
							}
						} else {
							want := &waCommon.MessageKey{RemoteJID: proto.String(tt.chat), ID: proto.String(tt.message), FromMe: proto.Bool(tt.fromMe)}
							if tt.participant != "" {
								want.Participant = proto.String(tt.participant)
							}
							if len(messages) != 1 || !proto.Equal(messages[0].GetKey(), want) {
								t.Fatalf("messages = %#v, want key %v", messages, want)
							}
						}
						return nil
					})
				if err != nil || calls != 1 {
					t.Fatalf("err=%v calls=%d, want one successful send", err, calls)
				}
			})
		}
	}
}

func TestChatReadStateSyncRejectsInvalidChatWithoutSending(t *testing.T) {
	for _, read := range []bool{false, true} {
		calls := 0
		err := sendChatReadStateSync(context.Background(), "", "", read, 0, false, "", func(context.Context, appstate.PatchInfo) error {
			calls++
			return nil
		})
		if err == nil || err.Error() != "invalid chat JID" || calls != 0 {
			t.Fatalf("read=%v: err=%v calls=%d", read, err, calls)
		}
	}
	if err := sendChatReadStateSync(context.Background(), "alice@s.whatsapp.net", "", false, 0, false, "", nil); err == nil || err.Error() != "nil chat read sync sender" {
		t.Fatalf("nil sender error = %v", err)
	}
}

func TestChatReadSyncStillRejectsEmptyIdentity(t *testing.T) {
	calls := 0
	err := sendChatReadSync(context.Background(), "alice@s.whatsapp.net", "", 42, false, "", func(context.Context, appstate.PatchInfo) error {
		calls++
		return nil
	})
	if err == nil || err.Error() != "empty message ID" || calls != 0 {
		t.Fatalf("err=%v calls=%d, want legacy validation without send", err, calls)
	}
}

func TestMarkChatAsReadEventStillRejectsInvalidRead(t *testing.T) {
	chat, err := types.ParseJID("alice@s.whatsapp.net")
	if err != nil {
		t.Fatal(err)
	}
	for _, action := range []*waSyncAction.MarkChatAsReadAction{
		{Read: proto.Bool(true)},
		{Read: proto.Bool(true), MessageRange: &waSyncAction.SyncActionMessageRange{}},
		{
			Read: proto.Bool(true),
			MessageRange: &waSyncAction.SyncActionMessageRange{
				LastMessageTimestamp: proto.Int64(0),
				Messages: []*waSyncAction.SyncActionMessage{{
					Key: &waCommon.MessageKey{RemoteJID: proto.String(chat.String()), ID: proto.String("latest"), FromMe: proto.Bool(false)},
				}},
			},
		},
	} {
		if event, ok := markChatAsReadEventFromEvent(&events.MarkChatAsRead{JID: chat, Action: action}); ok {
			t.Fatalf("invalid read accepted: %#v", event)
		}
	}
}

func TestChatReadSyncRejectsInvalidInputBeforeSending(t *testing.T) {
	called := false
	err := sendChatReadSync(context.Background(), "", "latest", 42, false, "", func(context.Context, appstate.PatchInfo) error {
		called = true
		return nil
	})
	if err == nil || called {
		t.Fatalf("err = %v, called = %v, want validation error without send", err, called)
	}
}
