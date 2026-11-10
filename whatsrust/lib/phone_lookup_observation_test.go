package main

import (
	"context"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestLookupPhoneObservationIsLocalRejectionData(t *testing.T) {
	const phone = "+15551234567"
	response := types.IsOnWhatsAppResponse{
		Query: phone, IsIn: true,
		PhoneNumber: types.NewJID("15559876543", types.DefaultUserServer),
		JID:         types.NewJID("99887766", types.HiddenUserServer),
	}
	calls := 0
	result, observation := lookupPhoneObserved(context.Background(), phone, func(_ context.Context, phones []string) ([]types.IsOnWhatsAppResponse, error) {
		calls++
		if len(phones) != 1 || phones[0] != phone {
			t.Fatal("request changed")
		}
		return []types.IsOnWhatsAppResponse{response}, nil
	})
	want := "Requested: \"+15551234567\"\nQuery: \"+15551234567\"\nIsIn: true\n" +
		"PhoneNumber.String: \"15559876543@s.whatsapp.net\"\nPhoneNumber.User: \"15559876543\"\nPhoneNumber.Server: \"s.whatsapp.net\"\nPhoneNumber.RawAgent: 0\nPhoneNumber.Device: 0\nPhoneNumber.Integrator: 0\n" +
		"JID.String: \"99887766@lid\"\nJID.User: \"99887766\"\nJID.Server: \"lid\"\nJID.RawAgent: 0\nJID.Device: 0\nJID.Integrator: 0"
	if result != (phoneLookupResult{status: 166}) || observation != want || calls != 1 {
		t.Fatalf("result=%+v observation=%q calls=%d", result, observation, calls)
	}
}

func TestLookupPhoneObservationPreservesGuardsAndBounds(t *testing.T) {
	const phone = "+15551234567"
	pn := types.NewJID(phone[1:], types.DefaultUserServer)
	lid := types.NewJID("99887766", types.HiddenUserServer)
	for _, tc := range []struct {
		name     string
		response types.IsOnWhatsAppResponse
		status   uint8
		jid      string
	}{
		{"exact success", types.IsOnWhatsAppResponse{Query: phone, IsIn: true, JID: pn}, 0, pn.String()},
		{"unregistered", types.IsOnWhatsAppResponse{Query: phone, JID: lid}, 1, ""},
		{"wrong query", types.IsOnWhatsAppResponse{Query: "+15559876543", IsIn: true, JID: pn}, 8, ""},
		{"LID only", types.IsOnWhatsAppResponse{Query: phone, IsIn: true, JID: lid}, 9, ""},
		{"oversized field", types.IsOnWhatsAppResponse{Query: phone, IsIn: true, JID: types.NewJID(strings.Repeat("1", 129), types.DefaultUserServer)}, 180, ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			result, observation := lookupPhoneObserved(context.Background(), phone, func(context.Context, []string) ([]types.IsOnWhatsAppResponse, error) {
				return []types.IsOnWhatsAppResponse{tc.response}, nil
			})
			if result != (phoneLookupResult{status: tc.status, jid: tc.jid}) || observation != "" {
				t.Fatalf("result=%+v observation=%q", result, observation)
			}
		})
	}
	result, observation := lookupPhoneObserved(context.Background(), phone, nil)
	if result.status != phoneLookupUnavailable || observation != "" {
		t.Fatal("nil query must stay unavailable")
	}
}

func TestLookupPhoneObservationEscapesTerminalControlAndShowsMetadata(t *testing.T) {
	const phone = "+15551234567"
	_, observation := lookupPhoneObserved(context.Background(), phone, func(context.Context, []string) ([]types.IsOnWhatsAppResponse, error) {
		return []types.IsOnWhatsAppResponse{{Query: phone, IsIn: true,
			PhoneNumber: types.JID{User: phone[1:], Server: types.DefaultUserServer, RawAgent: 3, Device: 2, Integrator: 1},
			JID:         types.NewJID("bad\x1b[2J\r\n", types.HiddenUserServer),
		}}, nil
	})
	for _, expected := range []string{"PhoneNumber.RawAgent: 3", "PhoneNumber.Device: 2", "PhoneNumber.Integrator: 1", `JID.User: "bad\x1b[2J\r\n"`} {
		if !strings.Contains(observation, expected) {
			t.Fatalf("missing %q in %q", expected, observation)
		}
	}
	for _, char := range observation {
		if char != '\n' && (char < 32 || char > 126) {
			t.Fatal("unsafe display control")
		}
	}
}
