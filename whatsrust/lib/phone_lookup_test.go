package main

import (
	"context"
	"errors"
	"testing"

	"go.mau.fi/whatsmeow/types"
)

func TestLookupPhoneRequiresServerConfirmedRequestedNumber(t *testing.T) {
	phone := "+15551234567"
	pn := types.NewJID("15551234567", types.DefaultUserServer)
	lid := types.NewJID("99887766", types.HiddenUserServer)
	tests := []struct {
		name      string
		phone     string
		responses []types.IsOnWhatsAppResponse
		err       error
		want      phoneLookupResult
		query     bool
	}{
		{name: "registered LID", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: pn, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "registered PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: pn, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()}, query: true},
		{name: "not registered", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: pn}}, want: phoneLookupResult{status: phoneLookupNotRegistered}, query: true},
		{name: "mismatched PN", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: phone, JID: lid, PhoneNumber: types.NewJID("15559876543", types.DefaultUserServer), IsIn: true}}, want: phoneLookupResult{status: phoneLookupFailed}, query: true},
		{name: "mismatched query", phone: phone, responses: []types.IsOnWhatsAppResponse{{Query: "+15559876543", JID: lid, PhoneNumber: pn, IsIn: true}}, want: phoneLookupResult{status: phoneLookupFailed}, query: true},
		{name: "missing reply", phone: phone, want: phoneLookupResult{status: phoneLookupFailed}, query: true},
		{name: "transport failure", phone: phone, err: errors.New("network"), want: phoneLookupResult{status: phoneLookupFailed}, query: true},
		{name: "missing plus", phone: "15551234567", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
		{name: "non-digit", phone: "+1555abc4567", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
		{name: "too short", phone: "+123456", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
		{name: "too long", phone: "+1234567890123456", want: phoneLookupResult{status: phoneLookupInvalidNumber}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			called := false
			got := lookupPhone(context.Background(), tc.phone, func(_ context.Context, phones []string) ([]types.IsOnWhatsAppResponse, error) {
				called = true
				if len(phones) != 1 || phones[0] != tc.phone {
					t.Fatalf("unexpected lookup: %v", phones)
				}
				return tc.responses, tc.err
			})
			if called != tc.query || got != tc.want {
				t.Fatalf("lookup = %+v, called = %t; want %+v, called = %t", got, called, tc.want, tc.query)
			}
		})
	}
}
