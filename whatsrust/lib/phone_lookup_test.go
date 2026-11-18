package main

import (
	"context"
	"errors"
	"fmt"
	"testing"

	"go.mau.fi/whatsmeow/types"
)

type phoneLookupTestCase struct {
	name      string
	phone     string
	responses []types.IsOnWhatsAppResponse
	err       error
	want      phoneLookupResult
	query     bool
}

func runPhoneLookupTests(t *testing.T, tests []phoneLookupTestCase) {
	t.Helper()
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			calls := 0
			got := lookupPhone(context.Background(), tc.phone, func(_ context.Context, phones []string) ([]types.IsOnWhatsAppResponse, error) {
				calls++
				if len(phones) != 1 || phones[0] != tc.phone {
					t.Fatalf("unexpected lookup: %v", phones)
				}
				return tc.responses, tc.err
			})
			wantCalls := 0
			if tc.query {
				wantCalls = 1
			}
			if calls != wantCalls || got != tc.want {
				t.Fatalf("lookup = %+v, calls = %d; want %+v, calls = %d", got, calls, tc.want, wantCalls)
			}
		})
	}
}

func TestLookupPhoneRequiresServerConfirmedRequestedNumber(t *testing.T) {
	phone := "+15551234567"
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	lid := types.NewJID("99887766", types.HiddenUserServer)
	tests := []phoneLookupTestCase{
		{name: "registered LID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: pn, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "registered PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: pn, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "registered legacy associated PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.NewJID(pn.User, types.LegacyUserServer), IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "registered legacy JID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID(pn.User, types.LegacyUserServer), IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "registered legacy device PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.JID{User: pn.User, Server: types.LegacyUserServer, Device: 2}, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "exact legacy PN despite different JID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID("15559876543", types.DefaultUserServer), PhoneNumber: types.NewJID(pn.User, types.LegacyUserServer), IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "different legacy PN stays rejected", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.NewJID("15559876543", types.LegacyUserServer), IsIn: true}}, want: phoneLookupResult{status: 7}, query: true},
		{name: "legacy PN with different integrator stays rejected", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.JID{User: pn.User, Server: types.LegacyUserServer, Integrator: 1}, IsIn: true}}, want: phoneLookupResult{status: 7}, query: true},
		{name: "not registered", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: pn}}, want: phoneLookupResult{status: phoneLookupNotRegistered}, query: true},
		{name: "registered device PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.JID{User: pn.User, Server: types.DefaultUserServer, Device: 2}, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "exact associated PN despite different JID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID("15559876543", types.DefaultUserServer), PhoneNumber: pn, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "exact JID despite different associated PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: pn, PhoneNumber: types.NewJID("15559876543", types.DefaultUserServer), IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		// Shape-only diagnostics append 9/10; existing ABI values 0-8 stay fixed.
		// Neither category confirms a recipient or proves number normalization.
		{name: "LID only without associated PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, IsIn: true}}, want: phoneLookupResult{status: 9}, query: true},
		{name: "mismatched associated PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.NewJID("15559876543", types.DefaultUserServer), IsIn: true}}, want: phoneLookupResult{status: 166}, query: true},
		{name: "mismatched JID PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID("15559876543", types.DefaultUserServer), IsIn: true}}, want: phoneLookupResult{status: 180}, query: true},
		// Comparison bits: PN digits/domain/integrator, then JID digits/domain/integrator.
		// The temporary observation remains a rejection with an empty recipient.
		{name: "same PN digits but different integrator", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.JID{User: pn.User, Server: types.DefaultUserServer, Integrator: 1}, IsIn: true}}, want: phoneLookupResult{status: 163}, query: true},
		{name: "same JID digits but different integrator", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.JID{User: pn.User, Server: types.DefaultUserServer, Integrator: 1}, IsIn: true}}, want: phoneLookupResult{status: 156}, query: true},
		{name: "same PN digits in rejected namespace alongside different PN JID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID("15559876543", types.DefaultUserServer), PhoneNumber: types.NewJID(pn.User, types.GroupServer), IsIn: true}}, want: phoneLookupResult{status: 181}, query: true},
		{name: "legacy comparison namespace alongside different PN JID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID("15559876543", types.DefaultUserServer), PhoneNumber: types.NewJID("15559876543", types.LegacyUserServer), IsIn: true}}, want: phoneLookupResult{status: 182}, query: true},
		{name: "missing identities", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, IsIn: true}}, want: phoneLookupResult{status: 7}, query: true},
		{name: "unexpected JID namespace", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: types.NewJID(pn.User, types.GroupServer), IsIn: true}}, want: phoneLookupResult{status: 7}, query: true},
		{name: "LID with unexpected associated namespace", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.NewJID(pn.User, types.GroupServer), IsIn: true}}, want: phoneLookupResult{status: 7}, query: true},
		{name: "mismatched query", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: "+15559876543", JID: lid, PhoneNumber: pn, IsIn: true}}, want: phoneLookupResult{status: 8}, query: true},
		{name: "matching response after unrelated query", phone: phone, responses: []types.IsOnWhatsAppResponse{
			{Query: "+15559876543", JID: lid, PhoneNumber: types.NewJID("15559876543", types.DefaultUserServer), IsIn: true},
			{Query: phone, JID: lid, PhoneNumber: pn, IsIn: true},
		}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "missing reply", phone: phone, want: phoneLookupResult{status: 6}, query: true},
		{name: "deadline exceeded", phone: phone, err: context.DeadlineExceeded, want: phoneLookupResult{status: 5}, query: true},
		{name: "wrapped deadline exceeded", phone: phone, err: fmt.Errorf("synthetic query: %w", context.DeadlineExceeded), want: phoneLookupResult{status: 5}, query: true},
		{name: "transport failure", phone: phone, err: errors.New("synthetic network failure"), want: phoneLookupResult{status: 4}, query: true},
		{name: "cancelled query remains generic failure", phone: phone, err: context.Canceled, want: phoneLookupResult{status: 4}, query: true},
		{name: "registered query without plus", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone[1:], JID: pn, IsIn: true}}, want: phoneLookupResult{status: 0, jid: pn.String()}, query: true},
		{name: "missing plus", phone: "15551234567", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
		{name: "non-digit", phone: "+1555abc4567", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
		{name: "too short", phone: "+123456", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
		{name: "too long", phone: "+1234567890123456", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
	}
	runPhoneLookupTests(t, tests)
}
