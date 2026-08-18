package main

import (
	"testing"
	"time"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func TestIncomingMessageDispatchesNormalizedEditBeforeOrdinaryMessage(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	event := &events.Message{
		Info: types.MessageInfo{
			MessageSource: types.MessageSource{Chat: chat, Sender: sender},
			ID:            "edit-action",
			Timestamp:     time.Unix(42, 0),
		},
		RawMessage: wrappedEditFixture("target-message", "replacement"),
		IsEdit:     true,
	}
	var actions []messageActionEvent
	ordinaryMessages := 0

	dispatchIncomingMessage(event, func(action messageActionEvent) {
		actions = append(actions, action)
	}, func(types.MessageInfo, *waE2E.Message, bool) {
		ordinaryMessages++
	})

	if len(actions) != 1 {
		t.Fatalf("dispatched %d actions, want 1", len(actions))
	}
	if actions[0].targetMessageID != "target-message" || actions[0].replacement != "replacement" {
		t.Fatalf("unexpected action: %#v", actions[0])
	}
	if ordinaryMessages != 0 {
		t.Fatalf("ordinary message path called %d times", ordinaryMessages)
	}
}

func TestIncomingMessageDispatchesViewOncePlaceholderFromAuthoritativeEnvelope(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	info := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, Sender: sender},
		ID:            "view-once-message",
		Timestamp:     time.Unix(42, 0),
	}
	viewOnce := func(message *waE2E.Message) *waE2E.Message {
		return &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: message}}
	}

	cases := []struct {
		name  string
		event *events.Message
	}{
		{
			name: "live raw envelope",
			event: &events.Message{
				Info:       info,
				Message:    &waE2E.Message{ImageMessage: &waE2E.ImageMessage{}},
				RawMessage: viewOnce(&waE2E.Message{ImageMessage: &waE2E.ImageMessage{}}),
			},
		},
		{
			name: "history source envelope",
			event: &events.Message{
				Info:    info,
				Message: &waE2E.Message{VideoMessage: &waE2E.VideoMessage{}},
				SourceWebMsg: &waWeb.WebMessageInfo{
					Message: viewOnce(&waE2E.Message{VideoMessage: &waE2E.VideoMessage{}}),
				},
			},
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			var actions []messageActionEvent
			var dispatched []struct {
				info    types.MessageInfo
				message *waE2E.Message
				isRetry bool
			}
			dispatchIncomingMessageWithDecrypt(testCase.event, func(action messageActionEvent) {
				actions = append(actions, action)
			}, func(info types.MessageInfo, message *waE2E.Message, isRetry bool) {
				dispatched = append(dispatched, struct {
					info    types.MessageInfo
					message *waE2E.Message
					isRetry bool
				}{info, message, isRetry})
			}, nil)

			if len(actions) != 0 || len(dispatched) != 1 {
				t.Fatalf("actions=%d dispatched=%d, want actions=0 dispatched=1", len(actions), len(dispatched))
			}
			if dispatched[0].info.ID != info.ID || dispatched[0].info.Chat != info.Chat || dispatched[0].info.Sender != info.Sender {
				t.Fatalf("message info was not preserved: %#v", dispatched[0].info)
			}
			if dispatched[0].isRetry || dispatched[0].message.GetConversation() != viewOnceUnavailablePlaceholder {
				t.Fatalf("unexpected placeholder dispatch: %#v", dispatched[0])
			}
		})
	}
}

func TestIncomingMessageDispatchKeepsOrdinaryMessageWithoutRawViewOnce(t *testing.T) {
	info := types.MessageInfo{ID: "ordinary-message"}
	ordinary := &waE2E.Message{Conversation: stringPointer("ordinary")}
	event := &events.Message{Info: info, Message: ordinary}

	actions := 0
	dispatched := 0
	var got *waE2E.Message
	dispatchIncomingMessageWithDecrypt(event, func(messageActionEvent) {
		actions++
	}, func(_ types.MessageInfo, message *waE2E.Message, isRetry bool) {
		dispatched++
		got = message
		if isRetry {
			t.Fatal("ordinary message was marked as a retry")
		}
	}, nil)

	if actions != 0 || dispatched != 1 || got != ordinary {
		t.Fatalf("actions=%d dispatched=%d got=%p, want actions=0 dispatched=1 original=%p", actions, dispatched, got, ordinary)
	}
}
