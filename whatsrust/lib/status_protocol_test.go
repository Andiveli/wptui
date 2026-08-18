package main

import (
	"os"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

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

func TestStatusProtocolContextOwnership(t *testing.T) {
	statusSource, err := os.ReadFile("status_protocol.go")
	if err != nil {
		t.Fatal(err)
	}
	mainSource, err := os.ReadFile("main.go")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(statusSource), "func isStatusProtocolContext(") {
		t.Fatal("status protocol context classifier is not owned by status_protocol.go")
	}
	if strings.Contains(string(mainSource), "func isStatusProtocolContext(") {
		t.Fatal("status protocol context classifier remains in main.go")
	}
}
