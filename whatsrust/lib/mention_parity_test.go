package main

import (
	"context"
	"reflect"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type parityContactStore struct {
	mentionContactStore
	contacts map[types.JID]types.ContactInfo
}

func (s parityContactStore) GetAllContacts(context.Context) (map[types.JID]types.ContactInfo, error) {
	return s.contacts, nil
}

func TestNormalAndOptimisticCallbacksPreserveMentionParity(t *testing.T) {
	mentioned := []string{"111@s.whatsapp.net", "222@lid"}
	contacts := []contactEntry{
		{jid: types.NewJID("111", types.DefaultUserServer), name: "Álvaro"},
		{jid: types.NewJID("222", types.HiddenUserServer), name: "李雷"},
	}
	message := &waE2E.Message{ExtendedTextMessage: &waE2E.ExtendedTextMessage{
		Text:        stringPtr("antes @111 dentro @222"),
		ContextInfo: &waE2E.ContextInfo{MentionedJID: mentioned},
	}}

	normalText, normalRanges, _ := replaceMentionedNamesWithContextRanges(
		context.Background(), message.GetExtendedTextMessage().GetText(), mentioned, contacts,
	)
	optimisticText := replaceMentionedNames(
		message.GetExtendedTextMessage().GetText(),
		message.GetExtendedTextMessage().GetContextInfo().GetMentionedJID(),
		contacts,
	)
	optimisticRanges := takePendingMentionRanges(optimisticText)
	if normalText != optimisticText || !reflect.DeepEqual(normalRanges, optimisticRanges) {
		t.Fatalf("callback mention parity mismatch: normal=(%q, %#v), optimistic=(%q, %#v)", normalText, normalRanges, optimisticText, optimisticRanges)
	}
}

func TestNormalAndOptimisticSendRoutesPreserveWireAndQuoteParity(t *testing.T) {
	contextInfo := quotedContextInfo("quoted-1", "sender@s.whatsapp.net", "chat@g.us")
	contextInfo.QuotedMessage = quotedTextMessage("quoted body")
	normal := contentToWaE2EMessage(MessageTypeText, "hello @111", []string{"111@lid"}, 0, "", nil, contextInfo, nil)
	optimistic := contentToWaE2EMessage(MessageTypeText, "hello @111", []string{"111@lid"}, 0, "", nil, contextInfo, nil)

	normalText := normal.GetExtendedTextMessage()
	optimisticText := optimistic.GetExtendedTextMessage()
	if normalText.GetText() != optimisticText.GetText() {
		t.Fatalf("wire text mismatch: normal=%q optimistic=%q", normalText.GetText(), optimisticText.GetText())
	}
	if !reflect.DeepEqual(normalText.GetContextInfo().GetMentionedJID(), optimisticText.GetContextInfo().GetMentionedJID()) {
		t.Fatalf("MentionedJID mismatch: normal=%v optimistic=%v", normalText.GetContextInfo().GetMentionedJID(), optimisticText.GetContextInfo().GetMentionedJID())
	}
	for _, field := range []struct{ name, normal, optimistic string }{
		{"stanza ID", normalText.GetContextInfo().GetStanzaID(), optimisticText.GetContextInfo().GetStanzaID()},
		{"participant", normalText.GetContextInfo().GetParticipant(), optimisticText.GetContextInfo().GetParticipant()},
		{"remote JID", normalText.GetContextInfo().GetRemoteJID(), optimisticText.GetContextInfo().GetRemoteJID()},
		{"quoted body", normalText.GetContextInfo().GetQuotedMessage().GetConversation(), optimisticText.GetContextInfo().GetQuotedMessage().GetConversation()},
	} {
		if field.normal != field.optimistic {
			t.Fatalf("%s mismatch: normal=%q optimistic=%q", field.name, field.normal, field.optimistic)
		}
	}
}

func TestNormalAndOptimisticProductionRoutesHaveWireAndCallbackParity(t *testing.T) {
	previousClient := client
	previousSend := requestSendMessage
	previousNormal := normalMessageCallback
	previousOptimistic := optimisticTextSentCallback
	previousObserve := observeTextCallback
	t.Cleanup(func() {
		client = previousClient
		requestSendMessage = previousSend
		normalMessageCallback = previousNormal
		optimisticTextSentCallback = previousOptimistic
		observeTextCallback = previousObserve
	})

	self := types.NewJID("111", types.DefaultUserServer)
	otherPN := types.NewJID("222", types.DefaultUserServer)
	otherLID := types.NewJID("222", types.HiddenUserServer)
	selfLID := types.NewJID("444", types.HiddenUserServer)
	client = &whatsmeow.Client{Store: &store.Device{
		ID:  &self,
		LID: selfLID,
		Contacts: parityContactStore{contacts: map[types.JID]types.ContactInfo{
			self:    {FullName: "Álvaro"},
			otherPN: {FullName: "李雷"},
		}},
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{otherLID: otherPN, selfLID: self},
			lidByPN: map[types.JID]types.JID{otherPN: otherLID, self: selfLID},
		},
	}}
	request := textSendRequest{
		messageType:   MessageTypeText,
		chat:          types.NewJID("999", types.DefaultUserServer),
		text:          "antes @111 dentro @222 y @444",
		mentionedJIDs: []string{"111@s.whatsapp.net", "222@lid", "444@lid"},
		quote: &textSendQuote{
			stanzaID:    "quoted-1",
			participant: "333@s.whatsapp.net",
			remoteJID:   "12345@g.us",
			content:     quotedTextMessage("quoted body"),
		},
		localSendID: 77,
	}

	var sent []*waE2E.Message
	var outputs []textCallbackOutput
	requestSendMessage = func(_ *whatsmeow.Client, _ context.Context, _ types.JID, message *waE2E.Message) (whatsmeow.SendResponse, error) {
		sent = append(sent, message)
		return whatsmeow.SendResponse{ID: "server-1"}, nil
	}
	observeTextCallback = func(output textCallbackOutput) {
		outputs = append(outputs, output)
	}
	normalMessageCallback = func(info types.MessageInfo, message *waE2E.Message, isSync bool) {
		HandleMessage(info, message, isSync)
	}
	optimisticTextSentCallback = func(localSendID uint64, info types.MessageInfo, message *waE2E.Message) {
		HandleOptimisticTextSent(localSendID, info, message)
	}

	if got := sendNormalTextRequest(request); got != 0 {
		t.Fatalf("normal route status = %d", got)
	}
	if got := sendOptimisticTextRequest(request); got != 0 {
		t.Fatalf("optimistic route status = %d", got)
	}
	if len(sent) != 2 {
		t.Fatalf("sender captured %d messages, want 2", len(sent))
	}
	if len(outputs) != 2 {
		t.Fatalf("callback count = %d, want 2", len(outputs))
	}
	optimisticOutput := outputs[1]
	optimisticOutput.localSendID = 0
	if len(outputs) != 2 || !reflect.DeepEqual(outputs[0], optimisticOutput) || outputs[0].localSendID != 0 || outputs[1].localSendID != 77 {
		t.Fatalf("callback parity mismatch: %#v", outputs)
	}
	if outputs[0].text != "antes @Álvaro dentro @李雷 y @Álvaro" {
		t.Fatalf("callback text = %q, want canonical Unicode names", outputs[0].text)
	}
	if !reflect.DeepEqual(outputs[0].ranges, []mentionRange{{Start: 6, End: 14}, {Start: 22, End: 29}, {Start: 32, End: 40}}) {
		t.Fatalf("callback ranges = %#v, want final UTF-8 ranges", outputs[0].ranges)
	}
	if !outputs[0].mentionsSelf || outputs[0].quoteID != "quoted-1" {
		t.Fatalf("callback semantic metadata = %#v", outputs[0])
	}
	normal, optimistic := sent[0].GetExtendedTextMessage(), sent[1].GetExtendedTextMessage()
	if normal.GetText() != "antes @111 dentro @222 y @444" || optimistic.GetText() != normal.GetText() || !reflect.DeepEqual(normal.GetContextInfo().GetMentionedJID(), []string{"111@s.whatsapp.net", "222@lid", "444@lid"}) || !reflect.DeepEqual(normal.GetContextInfo().GetMentionedJID(), optimistic.GetContextInfo().GetMentionedJID()) {
		t.Fatalf("route wire parity mismatch: normal=%v optimistic=%v", normal, optimistic)
	}
	if normal.GetContextInfo().GetStanzaID() != "quoted-1" || normal.GetContextInfo().GetParticipant() != "333@s.whatsapp.net" || normal.GetContextInfo().GetRemoteJID() != "12345@g.us" || normal.GetContextInfo().GetQuotedMessage().GetConversation() != "quoted body" || normal.GetContextInfo().GetStanzaID() != optimistic.GetContextInfo().GetStanzaID() || normal.GetContextInfo().GetParticipant() != optimistic.GetContextInfo().GetParticipant() || normal.GetContextInfo().GetRemoteJID() != optimistic.GetContextInfo().GetRemoteJID() || normal.GetContextInfo().GetQuotedMessage().GetConversation() != optimistic.GetContextInfo().GetQuotedMessage().GetConversation() {
		t.Fatal("route quote parity mismatch")
	}
}

func stringPtr(value string) *string { return &value }
