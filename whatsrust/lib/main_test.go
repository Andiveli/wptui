package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waCommon"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/proto/waWeb"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"
)

func profilePicturePNG(t *testing.T) []byte {
	t.Helper()
	var data bytes.Buffer
	img := image.NewRGBA(image.Rect(0, 0, 2, 2))
	img.Set(0, 0, color.White)
	if err := png.Encode(&data, img); err != nil {
		t.Fatal(err)
	}
	return data.Bytes()
}

func wrappedEditFixture(target, replacement string) *waE2E.Message {
	return &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
		ProtocolMessage: &waE2E.ProtocolMessage{
			Key:           &waCommon.MessageKey{ID: stringPointer(target)},
			Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
			EditedMessage: &waE2E.Message{Conversation: stringPointer(replacement)},
		},
	}}}
}

func TestFetchProfilePictureAvailablePreview(t *testing.T) {
	want := profilePicturePNG(t)
	lookupCalled := false
	downloadCalled := false
	outcome := fetchProfilePicture(context.Background(), "12345@g.us", func(_ context.Context, jid types.JID, params *whatsmeow.GetProfilePictureParams) (*types.ProfilePictureInfo, error) {
		lookupCalled = true
		if jid.Server != types.GroupServer || params == nil || !params.Preview {
			t.Fatalf("unexpected lookup request: jid=%s params=%#v", jid, params)
		}
		return &types.ProfilePictureInfo{URL: "https://temporary.invalid/avatar", ID: "picture-42", Type: "preview"}, nil
	}, func(_ context.Context, url string, limit int64) ([]byte, error) {
		downloadCalled = true
		if url != "https://temporary.invalid/avatar" || limit != profilePictureMaxSize {
			t.Fatalf("unexpected download request: url=%q limit=%d", url, limit)
		}
		return want, nil
	})

	if !lookupCalled || !downloadCalled || outcome.status != profilePictureStatusAvailable || outcome.pictureID != "picture-42" || outcome.pictureType != "preview" || !bytes.Equal(outcome.data, want) {
		t.Fatalf("unexpected outcome: %#v", outcome)
	}
}

func TestViewOnceWrappersBecomeUnavailablePlaceholders(t *testing.T) {
	for _, testCase := range []struct {
		name string
		wrap func(*waE2E.Message) *waE2E.Message
	}{
		{"v1", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"v2", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"v2 extension", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2Extension: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"ephemeral v2", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{ViewOnceMessageV2: &waE2E.FutureProofMessage{Message: message}}}}
		}},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			wrapped := testCase.wrap(&waE2E.Message{ImageMessage: &waE2E.ImageMessage{Caption: stringPointer("private media")}})
			placeholder, unavailable := unavailableViewOnceMessage(wrapped)
			if !unavailable {
				t.Fatal("view-once wrapper must be unavailable")
			}
			if got := placeholder.GetConversation(); got != viewOnceUnavailablePlaceholder {
				t.Fatalf("placeholder = %q, want %q", got, viewOnceUnavailablePlaceholder)
			}
			if placeholder.GetImageMessage() != nil || placeholder.GetVideoMessage() != nil {
				t.Fatal("view-once media must not reach the public message model")
			}
		})
	}
}

func TestViewOnceMessagesAreNotCachedForForwarding(t *testing.T) {
	resetForwardedSourcesForTest()
	t.Cleanup(resetForwardedSourcesForTest)
	chat := types.NewJID("chat", types.DefaultUserServer)
	info := types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: chat}, ID: "view-once"}
	message := &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{ImageMessage: &waE2E.ImageMessage{}}}}

	cacheForwardSource(info, message)

	if len(forwardedSources.entries) != 0 {
		t.Fatal("view-once media must not be cached for forwarding")
	}
}

func TestOrdinaryAndEphemeralMessagesRemainAvailable(t *testing.T) {
	ordinary := &waE2E.Message{Conversation: stringPointer("ordinary")}
	if got, unavailable := unavailableViewOnceMessage(ordinary); unavailable || got != ordinary {
		t.Fatal("ordinary message must remain unchanged")
	}
	ephemeral := &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: ordinary}}
	if got, unavailable := unavailableViewOnceMessage(ephemeral); unavailable || got != ephemeral {
		t.Fatal("ephemeral message must remain unchanged")
	}
}

func TestReactionDispatchOwnsPayloadInCUntilSynchronousCallbackReturns(t *testing.T) {
	source, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}

	body, ok := extractFunctionBody(string(source), "func dispatchReactionEvent(reaction reactionEvent)")
	if !ok {
		t.Fatal("dispatchReactionEvent function body not found in main.go")
	}

	for _, fragment := range []string{
		"func dispatchReactionEvent(reaction reactionEvent)",
		"if eventHandler.callback == nil",
		"(*C.ReactionEvent)(C.malloc(C.sizeof_ReactionEvent))",
		"C.callEventCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeReaction), data: unsafe.Pointer(payload)})",
		"C.free(unsafe.Pointer(payload))",
	} {
		if !strings.Contains(body, fragment) {
			t.Fatalf("reaction dispatch must contain %q", fragment)
		}
	}

	callback := strings.Index(body, "C.callEventCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeReaction), data: unsafe.Pointer(payload)})")
	freePayload := strings.Index(body, "C.free(unsafe.Pointer(payload))")
	if callback < 0 || freePayload < 0 || freePayload < callback {
		t.Fatal("reaction payload must remain C-owned until the callback returns")
	}
}

// extractFunctionBody returns the substring of code that spans the function
// whose signature starts with signaturePrefix, from the signature through the
// brace that closes the function body. Braces inside comments and inside
// string, raw-string, or rune literals never count, so unrelated earlier
// occurrences of the same statements in other functions cannot leak into the
// returned body.
func extractFunctionBody(code, signaturePrefix string) (string, bool) {
	sig := strings.Index(code, signaturePrefix)
	if sig < 0 {
		return "", false
	}
	open := strings.IndexByte(code[sig:], '{')
	if open < 0 {
		return "", false
	}
	depth := 1
	i := sig + open + 1
	for i < len(code) {
		switch code[i] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return code[sig : i+1], true
			}
		case '/':
			if i+1 < len(code) {
				switch code[i+1] {
				case '/':
					nl := strings.IndexByte(code[i:], '\n')
					if nl < 0 {
						return "", false
					}
					i += nl + 1
					continue
				case '*':
					end := strings.Index(code[i+2:], "*/")
					if end < 0 {
						return "", false
					}
					i += end + 4
					continue
				}
			}
		case '"':
			i++
			for i < len(code) && code[i] != '"' {
				if code[i] == '\\' {
					i++
				}
				i++
			}
		case '\'':
			i++
			for i < len(code) && code[i] != '\'' {
				if code[i] == '\\' {
					i++
				}
				i++
			}
		case '`':
			i++
			for i < len(code) && code[i] != '`' {
				i++
			}
		}
		i++
	}
	return "", false
}

func TestPinnedWhatsmeowMessageActionBuilders(t *testing.T) {
	var _ func(*whatsmeow.Client, types.JID, types.JID, types.MessageID, string) *waE2E.Message = (*whatsmeow.Client).BuildReaction
	var _ func(*whatsmeow.Client, types.JID, types.MessageID, *waE2E.Message) *waE2E.Message = (*whatsmeow.Client).BuildEdit
	var _ func(*whatsmeow.Client, types.JID, types.JID, types.MessageID) *waE2E.Message = (*whatsmeow.Client).BuildRevoke

	client := &whatsmeow.Client{}
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	messageID := types.MessageID("audit-message")

	t.Run("reaction", func(t *testing.T) {
		message := client.BuildReaction(chat, sender, messageID, "👍")
		if message.GetReactionMessage().GetText() != "👍" || message.GetReactionMessage().GetKey().GetID() != messageID {
			t.Fatal("reaction builder did not preserve target and text")
		}
	})
	t.Run("edit", func(t *testing.T) {
		replacement := &waE2E.Message{Conversation: stringPointer("replacement")}
		message := client.BuildEdit(chat, messageID, replacement)
		protocol := message.GetEditedMessage().GetMessage().GetProtocolMessage()
		if protocol.GetType() != waE2E.ProtocolMessage_MESSAGE_EDIT || protocol.GetKey().GetID() != messageID || protocol.GetEditedMessage().GetConversation() != "replacement" {
			t.Fatal("edit builder did not preserve protocol target and content")
		}
	})
	t.Run("revoke", func(t *testing.T) {
		message := client.BuildRevoke(chat, sender, messageID)
		if message.GetProtocolMessage().GetType() != waE2E.ProtocolMessage_REVOKE || message.GetProtocolMessage().GetKey().GetID() != messageID {
			t.Fatal("revoke builder did not preserve protocol target")
		}
	})
}

func TestMessageActionEventFromIncomingMessage(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	info := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, Sender: sender},
		ID:            "edit-action",
		Timestamp:     time.Unix(42, 0),
	}
	editProtocol := func(replacement *waE2E.Message) *waE2E.ProtocolMessage {
		return &waE2E.ProtocolMessage{
			Key:           &waCommon.MessageKey{ID: stringPointer("target-message")},
			Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
			EditedMessage: replacement,
		}
	}
	wrappedEdit := func(replacement *waE2E.Message) *waE2E.Message {
		return &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{
			ProtocolMessage: editProtocol(replacement),
		}}}
	}

	cases := []struct {
		name        string
		event       *events.Message
		wantBody    string
		wantAction  string
		wantTarget  string
		wantSuccess bool
	}{
		{
			name:        "live edit keeps envelope action identity and protocol target",
			event:       &events.Message{Info: info, RawMessage: wrappedEdit(&waE2E.Message{Conversation: stringPointer("conversation replacement")}), IsEdit: true},
			wantBody:    "conversation replacement",
			wantAction:  "edit-action",
			wantTarget:  "target-message",
			wantSuccess: true,
		},
		{
			name: "raw ephemeral edited-message wrapper with extended text",
			event: &events.Message{Info: info, RawMessage: &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{
				Message: wrappedEdit(&waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("extended replacement")}}),
			}}},
			wantBody:    "extended replacement",
			wantAction:  "edit-action",
			wantTarget:  "target-message",
			wantSuccess: true,
		},
		{
			name: "parsed history keeps source action identity and rewritten target",
			event: &events.Message{
				Info:         types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: sender}, ID: "target-message", Timestamp: time.Unix(43, 0)},
				RawMessage:   wrappedEdit(&waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("normalized replacement")}}),
				Message:      &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("normalized replacement")}},
				SourceWebMsg: &waWeb.WebMessageInfo{Key: &waCommon.MessageKey{ID: stringPointer("history-action")}},
			},
			wantBody:    "normalized replacement",
			wantAction:  "history-action",
			wantTarget:  "target-message",
			wantSuccess: true,
		},
		{
			name: "live normalized edit without raw envelope is ignored",
			event: &events.Message{
				Info:    info,
				Message: &waE2E.Message{Conversation: stringPointer("replacement")},
				IsEdit:  true,
			},
			wantSuccess: false,
		},
		{
			name:        "missing target key",
			event:       &events.Message{Info: info, RawMessage: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(), EditedMessage: &waE2E.Message{Conversation: stringPointer("replacement")}}}},
			wantSuccess: false,
		},
		{
			name:        "normalized edit without replacement",
			event:       &events.Message{Info: info, Message: &waE2E.Message{}, IsEdit: true},
			wantSuccess: false,
		},
		{
			name:        "ordinary text is not an edit",
			event:       &events.Message{Info: info, Message: &waE2E.Message{Conversation: stringPointer("ordinary")}},
			wantSuccess: false,
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			action, ok := messageActionEventFromIncomingMessage(testCase.event)
			if ok != testCase.wantSuccess {
				t.Fatalf("recognized = %v, want %v", ok, testCase.wantSuccess)
			}
			if !ok {
				return
			}
			if action.actionID != testCase.wantAction || action.targetMessageID != testCase.wantTarget || action.replacement != testCase.wantBody || action.chat != chat.String() || action.sender != sender.String() || action.occurredAt != testCase.event.Info.Timestamp.Unix() || action.kind != messageActionEdit {
				t.Fatalf("unexpected action: %#v", action)
			}
		})
	}
}

func TestMessageActionStructuralDiagnosticsAreSignalScopedAndRedacted(t *testing.T) {
	chat := types.NewJID("chat-secret", types.DefaultUserServer)
	sender := types.NewJID("sender-secret", types.DefaultUserServer)
	info := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, Sender: sender},
		ID:            "action-secret",
	}
	ordinary := &events.Message{Info: info, Message: &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")}}
	if line := messageActionStructuralLine(ordinary, "ordinary", ""); line != "" {
		t.Fatalf("ordinary message emitted diagnostic: %s", line)
	}

	wrappers := []struct {
		name string
		wrap func(*waE2E.Message) *waE2E.Message
	}{
		{"edited", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"device sent", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{DeviceSentMessage: &waE2E.DeviceSentMessage{Message: message}}
		}},
		{"bot invoke", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{BotInvokeMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"ephemeral", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{EphemeralMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"view once", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"view once v2", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"view once v2 extension", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{ViewOnceMessageV2Extension: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"lottie sticker", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{LottieStickerMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
		{"document caption", func(message *waE2E.Message) *waE2E.Message {
			return &waE2E.Message{DocumentWithCaptionMessage: &waE2E.FutureProofMessage{Message: message}}
		}},
	}
	for _, testCase := range wrappers {
		t.Run(testCase.name, func(t *testing.T) {
			event := &events.Message{Info: info, RawMessage: testCase.wrap(wrappedEditFixture("target-secret", "DO-NOT-LOG-BODY")), IsEdit: true}
			line := messageActionStructuralLine(event, "raw_classified", "")
			if strings.Count(line, "classifier=structural") != 1 {
				t.Fatalf("diagnostic count = %d, want 1: %s", strings.Count(line, "classifier=structural"), line)
			}
			if !strings.Contains(line, "replacement_variant=conversation") || !strings.Contains(line, "target_id=<id:") {
				t.Fatalf("diagnostic omitted structural fields: %s", line)
			}
			for _, secret := range []string{"DO-NOT-LOG-BODY", "target-secret", "action-secret", chat.String(), sender.String()} {
				if strings.Contains(line, secret) {
					t.Fatalf("diagnostic leaked %q: %s", secret, line)
				}
			}
			if _, ok := messageActionEventFromIncomingMessage(event); !ok {
				t.Fatal("wrapped edit was not classified")
			}
		})
	}

	missingTarget := &events.Message{Info: info, RawMessage: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
		Key:           &waCommon.MessageKey{},
		Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
		EditedMessage: &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")},
	}}, IsEdit: true}
	line := messageActionStructuralLine(missingTarget, "raw_miss", "missing_target_id")
	if !strings.Contains(line, "target_id=<missing>") || !strings.Contains(line, "protocol_key=true protocol_key_id=false") {
		t.Fatalf("missing target was not explicit: %s", line)
	}
}

func resetEventCensusForTest() {
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	eventCensus.nextSeq = 0
	eventCensus.entries = nil
}

func TestMessageActionEventCensusIsDisabledWithoutDebug(t *testing.T) {
	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "")
	resetEventCensusForTest()
	messageActionCensusDiagnostic(&events.Receipt{Type: types.ReceiptTypeRead})
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	if len(eventCensus.entries) != 0 {
		t.Fatalf("disabled census recorded %d entries", len(eventCensus.entries))
	}
}

func TestMessageActionEventCensusRedactsOrdinaryMessageAndLabelsNonMessage(t *testing.T) {
	chat := types.NewJID("chat-secret", types.DefaultUserServer)
	sender := types.NewJID("sender-secret", types.DefaultUserServer)
	event := &events.Message{
		Info:       types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: sender}, ID: "message-secret"},
		RawMessage: &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")},
		Message:    &waE2E.Message{Conversation: stringPointer("DO-NOT-LOG-BODY")},
	}
	line := messageActionCensusLine(event)
	for _, secret := range []string{"DO-NOT-LOG-BODY", "message-secret", chat.String(), sender.String()} {
		if strings.Contains(line, secret) {
			t.Fatalf("census leaked %q: %s", secret, line)
		}
	}
	for _, field := range []string{"event_type=events_message", "raw_kinds=conversation", "message_kinds=conversation", "info_id=<id:", "wrappers=raw"} {
		if !strings.Contains(line, field) {
			t.Fatalf("census omitted %q: %s", field, line)
		}
	}

	receipt := messageActionCensusLine(&events.Receipt{Type: types.ReceiptTypeRead})
	if !strings.Contains(receipt, "event_type=events_receipt subtype=receipt_read") {
		t.Fatalf("receipt census = %s", receipt)
	}
}

func TestMessageActionEventCensusIsBoundedAndOrdered(t *testing.T) {
	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "1")
	resetEventCensusForTest()
	for range messageActionCensusLimit + 1 {
		messageActionCensusDiagnostic(&events.Receipt{Type: types.ReceiptTypeRead})
	}
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	if len(eventCensus.entries) != messageActionCensusLimit {
		t.Fatalf("census length = %d, want %d", len(eventCensus.entries), messageActionCensusLimit)
	}
	if !strings.HasPrefix(eventCensus.entries[0], "census=event seq=2 ") || !strings.HasPrefix(eventCensus.entries[len(eventCensus.entries)-1], "census=event seq=101 ") {
		t.Fatalf("census order = first %q, last %q", eventCensus.entries[0], eventCensus.entries[len(eventCensus.entries)-1])
	}
}

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

func TestIncomingSecretEncryptedMessageEditDispatch(t *testing.T) {
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	editor := types.NewJID("0987654321", types.DefaultUserServer)
	secretEdit := func(target string, kind waE2E.SecretEncryptedMessage_SecretEncType) *waE2E.Message {
		return &waE2E.Message{SecretEncryptedMessage: &waE2E.SecretEncryptedMessage{
			TargetMessageKey: &waCommon.MessageKey{ID: stringPointer(target), Participant: stringPointer("target-author@s.whatsapp.net")},
			SecretEncType:    kind.Enum(),
			EncPayload:       []byte("ciphertext"),
			EncIV:            []byte("initial-vector"),
		}}
	}
	baseEvent := func(message *waE2E.Message) *events.Message {
		return &events.Message{Info: types.MessageInfo{
			MessageSource: types.MessageSource{Chat: chat, Sender: editor, IsGroup: true},
			ID:            "edit-action",
			Timestamp:     time.Unix(42, 0),
		}, RawMessage: message, Message: message}
	}

	cases := []struct {
		name             string
		event            *events.Message
		decrypt          decryptSecretEncryptedMessageFunc
		wantAction       bool
		wantReplacement  string
		wantOrdinary     int
		wantDecryptCalls int
	}{
		{
			name:  "conversation replacement dispatches one durable edit",
			event: baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			decrypt: func(_ context.Context, event *events.Message) (*waE2E.Message, error) {
				if event.Message.GetSecretEncryptedMessage() == nil {
					t.Fatal("decrypt did not receive the located secret envelope")
				}
				return &waE2E.Message{Conversation: stringPointer("replacement")}, nil
			},
			wantAction: true, wantReplacement: "replacement", wantDecryptCalls: 1,
		},
		{
			name:  "extended text replacement dispatches one durable edit",
			event: baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			decrypt: func(context.Context, *events.Message) (*waE2E.Message, error) {
				return &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{Text: stringPointer("replacement")}}, nil
			},
			wantAction: true, wantReplacement: "replacement", wantDecryptCalls: 1,
		},
		{
			name: "device sent wrapper locates the secret edit",
			event: baseEvent(&waE2E.Message{DeviceSentMessage: &waE2E.DeviceSentMessage{
				Message: secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT),
			}}),
			decrypt: func(context.Context, *events.Message) (*waE2E.Message, error) {
				return &waE2E.Message{Conversation: stringPointer("replacement")}, nil
			},
			wantAction: true, wantReplacement: "replacement", wantDecryptCalls: 1,
		},
		{
			name:             "other secret types remain ordinary",
			event:            baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_EVENT_EDIT)),
			wantOrdinary:     1,
			wantDecryptCalls: 0,
		},
		{
			name:             "missing target remains ordinary without decryption",
			event:            baseEvent(secretEdit("", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			wantOrdinary:     1,
			wantDecryptCalls: 0,
		},
		{
			name:  "missing original secret preserves the envelope without action",
			event: baseEvent(secretEdit("target-message", waE2E.SecretEncryptedMessage_MESSAGE_EDIT)),
			decrypt: func(context.Context, *events.Message) (*waE2E.Message, error) {
				return nil, whatsmeow.ErrOriginalMessageSecretNotFound
			},
			wantOrdinary: 1, wantDecryptCalls: 1,
		},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			decryptCalls := 0
			decrypt := testCase.decrypt
			if decrypt != nil {
				originalDecrypt := decrypt
				decrypt = func(ctx context.Context, event *events.Message) (*waE2E.Message, error) {
					decryptCalls++
					return originalDecrypt(ctx, event)
				}
			}
			var actions []messageActionEvent
			ordinaryMessages := 0
			originalMessage := testCase.event.Message
			dispatchIncomingMessageWithDecrypt(testCase.event, func(action messageActionEvent) {
				actions = append(actions, action)
			}, func(types.MessageInfo, *waE2E.Message, bool) {
				ordinaryMessages++
			}, decrypt)

			if len(actions) != map[bool]int{true: 1, false: 0}[testCase.wantAction] || ordinaryMessages != testCase.wantOrdinary || decryptCalls != testCase.wantDecryptCalls {
				t.Fatalf("actions=%d ordinary=%d decrypt_calls=%d", len(actions), ordinaryMessages, decryptCalls)
			}
			if !testCase.wantAction {
				if testCase.event.Message != originalMessage {
					t.Fatal("failed secret edit mutated the incoming envelope")
				}
				return
			}
			action := actions[0]
			if action.actionID != "edit-action" || action.targetMessageID != "target-message" || action.replacement != testCase.wantReplacement || action.chat != chat.String() || action.sender != editor.String() || action.occurredAt != 42 || action.kind != messageActionEdit {
				t.Fatalf("unexpected action: %#v", action)
			}
		})
	}
}

func TestSecretEditDiagnosticsAreRedacted(t *testing.T) {
	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "1")
	resetEventCensusForTest()
	event := &events.Message{Info: types.MessageInfo{ID: "action-secret"}, Message: &waE2E.Message{SecretEncryptedMessage: &waE2E.SecretEncryptedMessage{
		TargetMessageKey: &waCommon.MessageKey{ID: stringPointer("target-secret")},
		SecretEncType:    waE2E.SecretEncryptedMessage_MESSAGE_EDIT.Enum(),
		EncPayload:       []byte("DO-NOT-LOG-CIPHERTEXT"),
		EncIV:            []byte("DO-NOT-LOG-IV"),
	}}}

	if _, ok := messageActionEventFromSecretEncryptedMessage(event, func(context.Context, *events.Message) (*waE2E.Message, error) {
		return nil, whatsmeow.ErrOriginalMessageSecretNotFound
	}); ok {
		t.Fatal("failed decryption produced an action")
	}
	eventCensus.mu.Lock()
	defer eventCensus.mu.Unlock()
	if len(eventCensus.entries) != 1 {
		t.Fatalf("diagnostic entries = %d, want 1", len(eventCensus.entries))
	}
	line := eventCensus.entries[0]
	for _, secret := range []string{"action-secret", "target-secret", "DO-NOT-LOG-CIPHERTEXT", "DO-NOT-LOG-IV"} {
		if strings.Contains(line, secret) {
			t.Fatalf("diagnostic leaked %q: %s", secret, line)
		}
	}
	for _, field := range []string{"secret_edit_result=failed", "error_class=missing_original_secret", "secret_enc_type=message_edit", "secret_enc_type_number=2", "secret_payload_length=21", "secret_iv_length=13"} {
		if !strings.Contains(line, field) {
			t.Fatalf("diagnostic omitted %q: %s", field, line)
		}
	}
}

func TestOrdinaryActionBuildersRejectInvalidTargets(t *testing.T) {
	client := &whatsmeow.Client{}
	chat := types.NewJID("1234567890", types.DefaultUserServer)
	sender := types.NewJID("0987654321", types.DefaultUserServer)
	newsletter := types.NewJID("channel", types.NewsletterServer)

	if _, err := buildOrdinaryReaction(client, newsletter, sender, "message", "👍"); err == nil {
		t.Fatal("newsletter reaction was accepted as ordinary")
	}
	if _, err := buildOrdinaryReaction(client, chat, types.JID{}, "message", "👍"); err == nil {
		t.Fatal("reaction without sender was accepted")
	}
	if _, err := buildOrdinaryEdit(client, newsletter, "message", "replacement"); err == nil {
		t.Fatal("newsletter edit was accepted as ordinary")
	}
	if _, err := buildOrdinaryEdit(client, chat, "message", " \t"); err == nil {
		t.Fatal("blank edit was accepted")
	}
	if _, err := buildOrdinaryRevoke(client, newsletter, sender, "message"); err == nil {
		t.Fatal("newsletter revoke was accepted as ordinary")
	}
	if _, err := buildOrdinaryRevoke(client, chat, sender, ""); err == nil {
		t.Fatal("revoke without message ID was accepted")
	}
}

func TestReactionEventExtractsReactorAndSupportsRemoval(t *testing.T) {
	group := types.NewJID("12345", types.GroupServer)
	reactor := types.NewJID("reactor", types.DefaultUserServer)
	message := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
		Key:  &waCommon.MessageKey{ID: stringPointer("target"), Participant: stringPointer("target-author@s.whatsapp.net")},
		Text: stringPointer("👍"),
	}}

	for _, testCase := range []struct {
		name string
		text string
	}{
		{name: "adds or replaces a reaction", text: "👍"},
		{name: "empty text removes a reaction", text: ""},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			message.ReactionMessage.Text = stringPointer(testCase.text)
			event, ok := reactionEventFromMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: group, Sender: reactor}}, message)
			if !ok {
				t.Fatal("expected reaction event")
			}
			if event.chat != group.String() || event.targetMessageID != "target" || event.participant != reactor.String() || event.text != testCase.text || event.isFromMe {
				t.Fatalf("unexpected reaction event: %#v", event)
			}
		})
	}
}

func TestStatusProtocolReactionDiagnosticIncludesOnlyProtocolMetadata(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:     types.NewJID("mobile", types.HiddenUserServer),
		Sender:   types.NewJID("mobile", types.HiddenUserServer),
		IsFromMe: true,
	}}
	reactionFromMe := true
	message := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
		Key: &waCommon.MessageKey{
			RemoteJID:   stringPointer("status@broadcast"),
			Participant: stringPointer("author@s.whatsapp.net"),
			FromMe:      &reactionFromMe,
			ID:          stringPointer("status-id"),
		},
		Text: stringPointer("❤️"),
	}}

	line, ok := statusProtocolReactionDiagnostic(info, message)
	if !ok {
		t.Fatal("expected reaction diagnostic")
	}
	for _, expected := range []string{
		"status_protocol=reaction", "chat=mobile@lid", "sender=mobile@lid", "from_me=true",
		"remote_jid=status@broadcast", "participant=author@s.whatsapp.net", "key_from_me=true", "id=status-id",
		"emoji_codepoints=U+2764,U+FE0F", "emoji=❤️",
	} {
		if !strings.Contains(line, expected) {
			t.Fatalf("diagnostic missing %q: %s", expected, line)
		}
	}
}

func TestStatusProtocolDiagnosticEmissionRequiresDebug(t *testing.T) {
	var emitted []string
	emit := func(format string, args ...any) { emitted = append(emitted, fmt.Sprintf(format, args...)) }

	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "")
	emitStatusProtocolDiagnostic(emit, "status_protocol=reaction")
	if len(emitted) != 0 {
		t.Fatalf("disabled diagnostic emitted %#v", emitted)
	}

	t.Setenv("WPTUI_MESSAGE_ACTION_DEBUG", "1")
	emitStatusProtocolDiagnostic(emit, "status_protocol=reaction")
	if got, want := emitted, []string{"status_protocol=reaction"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("emitted = %#v, want %#v", got, want)
	}
}

func TestStatusProtocolReactionDiagnosticIgnoresOrdinaryReactions(t *testing.T) {
	message := &waE2E.Message{ReactionMessage: &waE2E.ReactionMessage{
		Key: &waCommon.MessageKey{
			RemoteJID:   stringPointer("chat@s.whatsapp.net"),
			Participant: stringPointer("author@s.whatsapp.net"),
			ID:          stringPointer("message-id"),
		},
		Text: stringPointer("👍"),
	}}
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:   types.NewJID("chat", types.DefaultUserServer),
		Sender: types.NewJID("reactor", types.DefaultUserServer),
	}}

	if line, ok := statusProtocolReactionDiagnostic(info, message); ok || line != "" {
		t.Fatalf("ordinary reaction emitted diagnostic: %q", line)
	}
}

func TestStatusProtocolContextDiagnosticIdentifiesStatusReferenceWithoutBody(t *testing.T) {
	info := types.MessageInfo{MessageSource: types.MessageSource{
		Chat:     types.NewJID("mobile", types.HiddenUserServer),
		Sender:   types.NewJID("mobile", types.HiddenUserServer),
		IsFromMe: true,
	}}
	statusSource := waE2E.ContextInfo_StatusSourceType(1)
	statusAttribution := waE2E.ContextInfo_StatusAttributionType(1)
	isGroupStatus := true
	message := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
		Text: stringPointer("private reply body"),
		ContextInfo: &waE2E.ContextInfo{
			QuotedMessage: &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
				Text: stringPointer("private quoted body"),
			}},
			StanzaID:              stringPointer("status-id"),
			Participant:           stringPointer("author@s.whatsapp.net"),
			RemoteJID:             stringPointer("status@broadcast"),
			PosterStatusID:        stringPointer("poster-status-id"),
			StatusSourceType:      &statusSource,
			StatusAttributionType: &statusAttribution,
			IsGroupStatus:         &isGroupStatus,
		},
	}}

	lines := statusProtocolContextDiagnostics(info, message)
	if len(lines) != 1 {
		t.Fatalf("diagnostic lines = %#v, want one", lines)
	}
	line := lines[0]
	for _, expected := range []string{
		"status_protocol=context", "chat=mobile@lid", "sender=mobile@lid", "from_me=true", "content=extended_text",
		"stanza_id=status-id", "participant=author@s.whatsapp.net", "remote_jid=status@broadcast", "poster_status_id=poster-status-id",
		"quoted_message_present=true", "quoted_message_kind=extended_text", "status_source_type_present=true", "status_attribution_type_present=true", "is_group_status_present=true", "is_group_status=true",
	} {
		if !strings.Contains(line, expected) {
			t.Fatalf("diagnostic missing %q: %s", expected, line)
		}
	}
	for _, private := range []string{"private reply body", "private quoted body"} {
		if strings.Contains(line, private) {
			t.Fatalf("diagnostic leaked body %q: %s", private, line)
		}
	}
}

func TestStatusProtocolContextDiagnosticsIgnoreOrdinaryQuotes(t *testing.T) {
	message := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
		ContextInfo: quotedContextInfo("ordinary-id", "author@s.whatsapp.net", "author@s.whatsapp.net"),
	}}
	if lines := statusProtocolContextDiagnostics(types.MessageInfo{}, message); len(lines) != 0 {
		t.Fatalf("ordinary quote emitted diagnostics: %#v", lines)
	}
}

func TestActionJIDAndBridgeSourceContracts(t *testing.T) {
	if _, err := parseActionJID(""); err == nil {
		t.Fatal("empty action JID was accepted")
	}
	source, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, fragment := range []string{"//export C_ReactToMessage", "//export C_EditMessage", "//export C_RevokeMessage"} {
		if !strings.Contains(string(source), fragment) {
			t.Fatalf("missing bridge export %q", fragment)
		}
	}
}

func TestSafeDownloadTargetRejectsUnsafePathsAndPreservesNestedPaths(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	for _, target := range []string{"../escape", "/absolute", "", "."} {
		if _, err := safeDownloadTarget(root, target); err == nil {
			t.Fatalf("unsafe target %q was accepted", target)
		}
	}
	if got, err := safeDownloadTarget(root, "nested/file.jpg"); err != nil || got != filepath.Join(root, "nested/file.jpg") {
		t.Fatalf("nested target = %q, %v", got, err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "escape")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape/file.jpg", []byte("data")); err == nil {
		t.Fatal("symlink escape was accepted")
	}
}

func TestWriteDownloadRejectsSymlinkedRootsAndFinalPaths(t *testing.T) {
	root, outside := t.TempDir(), t.TempDir()
	link := filepath.Join(t.TempDir(), "media")
	if err := os.Symlink(outside, link); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(link, "file.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked root was accepted")
	}
	if err := writeDownload(root, "nested/file.jpg", []byte("data")); err != nil {
		t.Fatal(err)
	}
	if got, err := os.ReadFile(filepath.Join(root, "nested/file.jpg")); err != nil || string(got) != "data" {
		t.Fatalf("download = %q, %v", got, err)
	}
	if err := os.Symlink(filepath.Join(outside, "escape.jpg"), filepath.Join(root, "escape.jpg")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked final path was replaced")
	}
}

func TestWriteDownloadNeverAcceptsExistingOrPartialRegularFilesAndCleansFailedWrites(t *testing.T) {
	root := t.TempDir()
	destination := filepath.Join(root, "partial.jpg")
	if err := os.WriteFile(destination, []byte("partial"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "partial.jpg", []byte("complete")); err == nil {
		t.Fatal("existing partial regular file was accepted")
	}
	got, err := os.ReadFile(destination)
	if err != nil || string(got) != "partial" {
		t.Fatalf("existing file was changed: %q, %v", got, err)
	}
	if _, err := os.Stat(filepath.Join(root, ".partial.jpg.part")); !os.IsNotExist(err) {
		t.Fatalf("rename failure left a stale temporary file: %v", err)
	}
	if err := os.Remove(destination); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "partial.jpg", []byte("retry")); err != nil {
		t.Fatalf("retry after destination removal failed: %v", err)
	}
}

func TestWriteDownloadCleansTemporaryFileAfterWriteFailure(t *testing.T) {
	root := t.TempDir()
	writeFailure := errors.New("injected write failure")

	err := writeDownloadWithWriter(root, "failed.jpg", []byte("data"), func(*os.File, []byte) (int, error) {
		return 0, writeFailure
	})

	if !errors.Is(err, writeFailure) {
		t.Fatalf("write error = %v, want injected failure", err)
	}
	if _, err := os.Stat(filepath.Join(root, ".failed.jpg.part")); !os.IsNotExist(err) {
		t.Fatalf("failed write left a stale temporary file: %v", err)
	}
}

func TestBuildFileMessageMapsEveryFileKind(t *testing.T) {
	t.Parallel()

	directory := t.TempDir()
	contextInfo := &waE2E.ContextInfo{StanzaID: stringPointer("quoted-message")}
	uploaded := whatsmeow.UploadResponse{
		URL:           "https://upload.example/media",
		DirectPath:    "/v/t62.7118-24/media",
		MediaKey:      []byte("media-key"),
		FileEncSHA256: []byte("encrypted-hash"),
		FileSHA256:    []byte("plain-hash"),
	}

	testCases := []struct {
		name      string
		kind      uint8
		extension string
		mediaType whatsmeow.MediaType
		assert    func(*testing.T, *waE2E.Message)
	}{
		{
			name:      "image",
			kind:      FileTypeImage,
			extension: ".png",
			mediaType: whatsmeow.MediaImage,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.ImageMessage == nil || message.ImageMessage.Caption == nil {
					t.Fatal("expected image message with caption")
				}
				assertMediaFields(t, message.ImageMessage.GetURL(), message.ImageMessage.GetDirectPath(), message.ImageMessage.GetMimetype(), message.ImageMessage.GetFileLength(), message.ImageMessage.GetContextInfo(), message.ImageMessage.GetMediaKey(), message.ImageMessage.GetFileSHA256(), message.ImageMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "video",
			kind:      FileTypeVideo,
			extension: ".mp4",
			mediaType: whatsmeow.MediaVideo,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.VideoMessage == nil || message.VideoMessage.Caption == nil {
					t.Fatal("expected video message with caption")
				}
				assertMediaFields(t, message.VideoMessage.GetURL(), message.VideoMessage.GetDirectPath(), message.VideoMessage.GetMimetype(), message.VideoMessage.GetFileLength(), message.VideoMessage.GetContextInfo(), message.VideoMessage.GetMediaKey(), message.VideoMessage.GetFileSHA256(), message.VideoMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "audio",
			kind:      FileTypeAudio,
			extension: ".ogg",
			mediaType: whatsmeow.MediaAudio,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.AudioMessage == nil {
					t.Fatal("expected audio message")
				}
				assertMediaFields(t, message.AudioMessage.GetURL(), message.AudioMessage.GetDirectPath(), message.AudioMessage.GetMimetype(), message.AudioMessage.GetFileLength(), message.AudioMessage.GetContextInfo(), message.AudioMessage.GetMediaKey(), message.AudioMessage.GetFileSHA256(), message.AudioMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "document",
			kind:      FileTypeDocument,
			extension: ".txt",
			mediaType: whatsmeow.MediaDocument,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.DocumentMessage == nil || message.DocumentMessage.Caption == nil {
					t.Fatal("expected document message with caption")
				}
				assertMediaFields(t, message.DocumentMessage.GetURL(), message.DocumentMessage.GetDirectPath(), message.DocumentMessage.GetMimetype(), message.DocumentMessage.GetFileLength(), message.DocumentMessage.GetContextInfo(), message.DocumentMessage.GetMediaKey(), message.DocumentMessage.GetFileSHA256(), message.DocumentMessage.GetFileEncSHA256(), uploaded)
			},
		},
		{
			name:      "sticker",
			kind:      FileTypeSticker,
			extension: ".webp",
			mediaType: whatsmeow.MediaImage,
			assert: func(t *testing.T, message *waE2E.Message) {
				t.Helper()
				if message.StickerMessage == nil {
					t.Fatal("expected sticker message")
				}
				assertMediaFields(t, message.StickerMessage.GetURL(), message.StickerMessage.GetDirectPath(), message.StickerMessage.GetMimetype(), message.StickerMessage.GetFileLength(), message.StickerMessage.GetContextInfo(), message.StickerMessage.GetMediaKey(), message.StickerMessage.GetFileSHA256(), message.StickerMessage.GetFileEncSHA256(), uploaded)
			},
		},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			path := filepath.Join(directory, "attachment"+testCase.extension)
			data := []byte(testCase.name + " payload")
			if err := os.WriteFile(path, data, 0o600); err != nil {
				t.Fatal(err)
			}

			caption := "caption"
			var uploadedAs whatsmeow.MediaType
			message, err := buildFileMessage(context.Background(), testCase.kind, path, &caption, contextInfo, func(_ context.Context, actualData []byte, mediaType whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
				if string(actualData) != string(data) {
					t.Fatalf("uploaded data = %q, want %q", actualData, data)
				}
				uploadedAs = mediaType
				return uploaded, nil
			})
			if err != nil {
				t.Fatal(err)
			}
			if uploadedAs != testCase.mediaType {
				t.Fatalf("upload media type = %q, want %q", uploadedAs, testCase.mediaType)
			}
			testCase.assert(t, message)
			if actualCaption := messageCaption(message); actualCaption != nil && *actualCaption != caption {
				t.Fatalf("caption = %q, want %q", *actualCaption, caption)
			}
		})
	}
}

func TestBuildFileMessageKeepsNilCaptionNil(t *testing.T) {
	t.Parallel()

	for _, testCase := range []struct {
		name string
		kind uint8
		ext  string
	}{
		{name: "image", kind: FileTypeImage, ext: ".png"},
		{name: "video", kind: FileTypeVideo, ext: ".mp4"},
		{name: "document", kind: FileTypeDocument, ext: ".txt"},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "attachment"+testCase.ext)
			if err := os.WriteFile(path, []byte("media payload"), 0o600); err != nil {
				t.Fatal(err)
			}

			message, err := buildFileMessage(context.Background(), testCase.kind, path, nil, &waE2E.ContextInfo{}, func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
				return whatsmeow.UploadResponse{}, nil
			})
			if err != nil {
				t.Fatal(err)
			}
			if caption := messageCaption(message); caption != nil {
				t.Fatalf("caption = %q, want nil", *caption)
			}
		})
	}
}

func TestQuotedMessageFromContentPreservesTextAndFileKindsWithoutUpload(t *testing.T) {
	quotedText := quotedTextMessage("quoted text")
	if quotedText.GetConversation() != "quoted text" {
		t.Fatalf("quoted text = %#v", quotedText)
	}

	quotedImage := quotedFileMessage(FileTypeImage, "caption")
	if quotedImage.GetImageMessage() == nil || quotedImage.GetImageMessage().GetCaption() != "caption" {
		t.Fatalf("quoted image = %#v", quotedImage)
	}
	if quotedFileMessage(99, "") != nil {
		t.Fatal("unknown quoted file kind must be omitted")
	}
}

func TestQuotedContextInfoPreservesOriginalChatForStatusAndOrdinaryReplies(t *testing.T) {
	status := quotedContextInfo("status-id", "alice@s.whatsapp.net", "status@broadcast")
	if status.GetStanzaID() != "status-id" || status.GetParticipant() != "alice@s.whatsapp.net" || status.GetRemoteJID() != "status@broadcast" {
		t.Fatalf("status quote context = %+v, want status target attribution", status)
	}

	ordinary := quotedContextInfo("message-id", "bob@s.whatsapp.net", "bob@s.whatsapp.net")
	if ordinary.GetRemoteJID() != "bob@s.whatsapp.net" {
		t.Fatalf("ordinary quote remote JID = %q, want original chat", ordinary.GetRemoteJID())
	}
}

func TestReactionRequestSeparatesTargetFromDestination(t *testing.T) {
	target := types.NewJID("status", types.BroadcastServer)
	destination := types.NewJID("alice", types.DefaultUserServer)
	sender := types.NewJID("alice", types.DefaultUserServer)

	request, err := newReactionRequest(target, destination, sender, "status-id", "❤️")
	if err != nil {
		t.Fatal(err)
	}
	if request.target != target || request.destination != destination || request.sender != sender {
		t.Fatalf("reaction request = %+v, want distinct status target and inbox destination", request)
	}

	message, err := buildOrdinaryReaction(&whatsmeow.Client{}, request.target, request.sender, request.id, request.reaction)
	if err != nil {
		t.Fatal(err)
	}
	if key := message.GetReactionMessage().GetKey(); key.GetRemoteJID() != target.String() || key.GetParticipant() != sender.String() || key.GetID() != "status-id" {
		t.Fatalf("reaction key = %+v, want status target key", key)
	}
}

func assertMediaFields(t *testing.T, url, directPath, mimetype string, length uint64, contextInfo *waE2E.ContextInfo, mediaKey, fileSHA256, fileEncSHA256 []byte, uploaded whatsmeow.UploadResponse) {
	t.Helper()
	if url != uploaded.URL || directPath != uploaded.DirectPath {
		t.Fatalf("upload metadata = (%q, %q), want (%q, %q)", url, directPath, uploaded.URL, uploaded.DirectPath)
	}
	if mimetype == "" {
		t.Fatal("expected MIME type")
	}
	if length == 0 {
		t.Fatal("expected non-zero file length")
	}
	if string(mediaKey) != string(uploaded.MediaKey) || string(fileSHA256) != string(uploaded.FileSHA256) || string(fileEncSHA256) != string(uploaded.FileEncSHA256) {
		t.Fatal("expected upload hashes and media key to be preserved")
	}
	if contextInfo == nil || contextInfo.GetStanzaID() != "quoted-message" {
		t.Fatalf("context info = %#v, want quoted-message", contextInfo)
	}
}

func stringPointer(value string) *string {
	return &value
}

func messageCaption(message *waE2E.Message) *string {
	switch {
	case message.ImageMessage != nil:
		return message.ImageMessage.Caption
	case message.VideoMessage != nil:
		return message.VideoMessage.Caption
	case message.DocumentMessage != nil:
		return message.DocumentMessage.Caption
	default:
		return nil
	}
}

func TestMessageActionEventFromMessage(t *testing.T) {
	baseInfo := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: types.NewJID("chat", types.DefaultUserServer), Sender: types.NewJID("sender", types.DefaultUserServer)},
		ID:            "action-id",
		Timestamp:     time.Unix(100, 0),
	}
	tests := []struct {
		name      string
		message   *waE2E.Message
		wantKind  uint8
		wantText  string
		wantTime  int64
		wantMatch bool
	}{
		{
			name: "edited wrapper preserves target replacement and protocol timestamp",
			message: &waE2E.Message{EditedMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
				Type:          waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(),
				Key:           &waCommon.MessageKey{ID: stringPointer("target")},
				EditedMessage: &waE2E.Message{Conversation: stringPointer("replacement")},
				TimestampMS:   int64Pointer(42_000),
			}}}},
			wantKind: messageActionEdit, wantText: "replacement", wantTime: 42, wantMatch: true,
		},
		{
			name: "revoke preserves target and envelope timestamp",
			message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{
				Type: waE2E.ProtocolMessage_REVOKE.Enum(), Key: &waCommon.MessageKey{ID: stringPointer("target")},
			}},
			wantKind: messageActionDelete, wantTime: 100, wantMatch: true,
		},
		{name: "unsupported protocol is ignored", message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{}}, wantMatch: false},
		{name: "edit without replacement is ignored", message: &waE2E.Message{ProtocolMessage: &waE2E.ProtocolMessage{Type: waE2E.ProtocolMessage_MESSAGE_EDIT.Enum(), Key: &waCommon.MessageKey{ID: stringPointer("target")}}}, wantMatch: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			action, ok := messageActionEventFromMessage(baseInfo, tt.message)
			if ok != tt.wantMatch {
				t.Fatalf("matched = %v, want %v", ok, tt.wantMatch)
			}
			if !ok {
				return
			}
			if action.actionID != "action-id" || action.targetMessageID != "target" || action.kind != tt.wantKind || action.replacement != tt.wantText || action.occurredAt != tt.wantTime {
				t.Fatalf("action = %#v", action)
			}
		})
	}
}

func TestPrepareForwardMessagePreservesTextAndFilePayloads(t *testing.T) {
	text := &waE2E.Message{Conversation: stringPointer("hello")}
	forwarded, err := prepareForwardMessage(text, false)
	if err != nil {
		t.Fatalf("prepare text forward: %v", err)
	}
	if forwarded.GetExtendedTextMessage().GetText() != "hello" || !forwarded.GetExtendedTextMessage().GetContextInfo().GetIsForwarded() {
		t.Fatalf("text forwarding envelope = %#v", forwarded)
	}

	media := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{URL: stringPointer("https://media.example"), DirectPath: stringPointer("/media")}}
	forwarded, err = prepareForwardMessage(media, false)
	if err != nil {
		t.Fatalf("prepare file forward: %v", err)
	}
	if forwarded.GetImageMessage().GetDirectPath() != "/media" || !forwarded.GetImageMessage().GetContextInfo().GetIsForwarded() {
		t.Fatalf("file forwarding envelope lost media payload: %#v", forwarded)
	}
}

func TestPrepareForwardMessageAppliesMobileForwardingContextSemantics(t *testing.T) {
	stanzaID := "quoted-message"
	score := uint32(4)
	other := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
		Text: stringPointer("other person's message"),
		ContextInfo: &waE2E.ContextInfo{
			StanzaID:        &stanzaID,
			ForwardingScore: &score,
			IsForwarded:     proto.Bool(true),
		},
	}}

	forwarded, err := prepareForwardMessage(other, false)
	if err != nil {
		t.Fatalf("prepare other sender forward: %v", err)
	}
	context := forwarded.GetExtendedTextMessage().GetContextInfo()
	if !context.GetIsForwarded() || context.GetForwardingScore() != 5 || context.GetStanzaID() != stanzaID {
		t.Fatalf("other sender context = %#v", context)
	}

	repeated, err := prepareForwardMessage(forwarded, false)
	if err != nil {
		t.Fatalf("prepare repeatedly forwarded message: %v", err)
	}
	if context := repeated.GetExtendedTextMessage().GetContextInfo(); !context.GetIsForwarded() || context.GetForwardingScore() != 5 || context.GetStanzaID() != stanzaID {
		t.Fatalf("capped frequently-forwarded context = %#v", context)
	}

	own := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{
		DirectPath:  stringPointer("/media"),
		ContextInfo: &waE2E.ContextInfo{StanzaID: &stanzaID, ForwardingScore: &score, IsForwarded: proto.Bool(true)},
	}}
	forwarded, err = prepareForwardMessage(own, true)
	if err != nil {
		t.Fatalf("prepare own forward: %v", err)
	}
	context = forwarded.GetImageMessage().GetContextInfo()
	if context.GetIsForwarded() || context.GetForwardingScore() != 0 || context.GetStanzaID() != stanzaID || forwarded.GetImageMessage().GetDirectPath() != "/media" {
		t.Fatalf("own sender context = %#v, message = %#v", context, forwarded)
	}
}

func TestSourceOwnedByCurrentUserUsesMessageMetadataAndClientIdentity(t *testing.T) {
	self := types.NewJID("self", types.DefaultUserServer)
	other := types.NewJID("other", types.DefaultUserServer)
	if !sourceOwnedByCurrentUser(true, other, self) {
		t.Fatal("source metadata marking the message as own was ignored")
	}
	if !sourceOwnedByCurrentUser(false, self, self) {
		t.Fatal("source sender matching the current client identity was ignored")
	}
	if sourceOwnedByCurrentUser(false, other, self) {
		t.Fatal("other sender was incorrectly treated as current user")
	}
}

func TestNewForwardRequestRejectsBroadcastAndInvalidDestinations(t *testing.T) {
	chat := types.NewJID("source", types.DefaultUserServer)
	sender := types.NewJID("source", types.DefaultUserServer)
	if _, err := newForwardRequest(chat.String(), sender.String(), "message", []string{types.StatusBroadcastJID.String()}); err == nil {
		t.Fatal("broadcast destination was accepted")
	}
	if _, err := newForwardRequest(types.StatusBroadcastJID.String(), sender.String(), "message", []string{chat.String()}); err == nil {
		t.Fatal("status source was accepted")
	}
	newsletter := types.NewJID("channel", types.NewsletterServer)
	if _, err := newForwardRequest(chat.String(), sender.String(), "message", []string{newsletter.String()}); err == nil {
		t.Fatal("newsletter destination was accepted")
	}
	if _, err := newForwardRequest(newsletter.String(), sender.String(), "message", []string{chat.String()}); err == nil {
		t.Fatal("newsletter source was accepted")
	}
	if _, err := newForwardRequest(chat.String(), sender.String(), "", []string{chat.String()}); err == nil {
		t.Fatal("empty source ID was accepted")
	}
	request, err := newForwardRequest(chat.String(), sender.String(), "message", []string{chat.String()})
	if err != nil || len(request.destinations) != 1 {
		t.Fatalf("forward to source chat must be allowed: request=%#v err=%v", request, err)
	}
}

func TestForwardSourceCacheEvictsAndInvalidatesDeletedSources(t *testing.T) {
	resetForwardedSourcesForTest()
	t.Cleanup(resetForwardedSourcesForTest)
	chat := types.NewJID("source", types.DefaultUserServer)
	sender := types.NewJID("sender", types.DefaultUserServer)
	for index := 0; index <= maxForwardSources; index++ {
		id := types.MessageID(fmt.Sprintf("message-%d", index))
		cacheForwardSource(types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: sender}, ID: id}, &waE2E.Message{Conversation: stringPointer("payload")})
	}
	first := forwardSourceKey(chat, sender, "message-0")
	last := forwardSourceKey(chat, sender, types.MessageID(fmt.Sprintf("message-%d", maxForwardSources)))
	forwardedSources.mu.Lock()
	_, firstExists := forwardedSources.entries[first]
	_, lastExists := forwardedSources.entries[last]
	entryCount := len(forwardedSources.entries)
	forwardedSources.mu.Unlock()
	if firstExists || !lastExists || entryCount != maxForwardSources {
		t.Fatalf("cache eviction failed: first=%t last=%t count=%d", firstExists, lastExists, entryCount)
	}
	removeForwardSources(chat.String(), "message-"+fmt.Sprint(maxForwardSources))
	forwardedSources.mu.Lock()
	_, lastExists = forwardedSources.entries[last]
	forwardedSources.mu.Unlock()
	if lastExists {
		t.Fatal("deleted source remained forwardable in cache")
	}
}

func TestForwardSourceBytesSurviveCacheReset(t *testing.T) {
	text := &waE2E.Message{Conversation: stringPointer("historical")}
	raw, err := proto.Marshal(text)
	if err != nil {
		t.Fatalf("marshal source: %v", err)
	}
	resetForwardedSourcesForTest()
	restored, reason := forwardSourceFromBytes(raw)
	if reason != forwardFailureNone || restored.GetConversation() != "historical" {
		t.Fatalf("historical source recovery = %#v, %v", restored, reason)
	}
	file := &waE2E.Message{ImageMessage: &waE2E.ImageMessage{DirectPath: stringPointer("/media")}}
	raw, err = proto.Marshal(file)
	if err != nil {
		t.Fatalf("marshal file source: %v", err)
	}
	restored, reason = forwardSourceFromBytes(raw)
	if reason != forwardFailureNone || restored.GetImageMessage().GetDirectPath() != "/media" {
		t.Fatalf("historical file recovery = %#v, %v", restored, reason)
	}
}

func TestForwardingStateExtractsEverySupportedMessageContext(t *testing.T) {
	forwarded := proto.Bool(true)
	score := uint32(5)
	context := &waE2E.ContextInfo{IsForwarded: forwarded, ForwardingScore: &score}
	cases := []struct {
		name    string
		message *waE2E.Message
	}{
		{"extended text", &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{ContextInfo: context}}},
		{"image", &waE2E.Message{ImageMessage: &waE2E.ImageMessage{ContextInfo: context}}},
		{"video", &waE2E.Message{VideoMessage: &waE2E.VideoMessage{ContextInfo: context}}},
		{"audio", &waE2E.Message{AudioMessage: &waE2E.AudioMessage{ContextInfo: context}}},
		{"document", &waE2E.Message{DocumentMessage: &waE2E.DocumentMessage{ContextInfo: context}}},
		{"sticker", &waE2E.Message{StickerMessage: &waE2E.StickerMessage{ContextInfo: context}}},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			state := forwardingStateFromMessage(testCase.message)
			if !state.isForwarded || state.score != 5 {
				t.Fatalf("forwarding state = %#v", state)
			}
		})
	}

	t.Run("conversation has no forwarding context", func(t *testing.T) {
		state := forwardingStateFromMessage(&waE2E.Message{Conversation: stringPointer("text")})
		if state != (forwardingState{}) {
			t.Fatalf("conversation forwarding state = %#v", state)
		}
	})
}

func int64Pointer(value int64) *int64 { return &value }
