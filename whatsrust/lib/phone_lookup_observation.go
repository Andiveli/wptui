package main

import (
	"context"
	"fmt"
	"strconv"

	"go.mau.fi/whatsmeow/types"
)

// lookupPhoneObserved keeps local inspection separate from the accepted recipient.
// It reuses the same query and validation, with no additional server request.
func lookupPhoneObserved(ctx context.Context, phone string, query phoneLookupQuery) (phoneLookupResult, string) {
	if query == nil {
		return lookupPhone(ctx, phone, nil), ""
	}
	var responses []types.IsOnWhatsAppResponse
	result := lookupPhone(ctx, phone, func(ctx context.Context, phones []string) ([]types.IsOnWhatsAppResponse, error) {
		var err error
		responses, err = query(ctx, phones)
		return responses, err
	})
	if result.status < 128 || result.status > 191 {
		return result, ""
	}
	for _, response := range responses {
		if response.Query != phone && response.Query != phone[1:] {
			continue
		}
		// The existing lookup selected this first matched response. Bound all
		// strings before formatting, and escape controls/non-ASCII for the TUI.
		for _, value := range []string{response.Query, response.PhoneNumber.User, response.PhoneNumber.Server, response.JID.User, response.JID.Server} {
			if len(value) > 128 {
				return result, ""
			}
		}
		text := fmt.Sprintf("Requested: %s\nQuery: %s\nIsIn: %t\n%s\n%s",
			strconv.QuoteToASCII(phone), strconv.QuoteToASCII(response.Query), response.IsIn,
			phoneLookupIdentityText("PhoneNumber", response.PhoneNumber),
			phoneLookupIdentityText("JID", response.JID))
		if len(text) > 8192 {
			return result, ""
		}
		return result, text
	}
	return result, ""
}

func phoneLookupIdentityText(label string, jid types.JID) string {
	return fmt.Sprintf("%s.String: %s\n%s.User: %s\n%s.Server: %s\n%s.RawAgent: %d\n%s.Device: %d\n%s.Integrator: %d",
		label, strconv.QuoteToASCII(jid.String()), label, strconv.QuoteToASCII(jid.User),
		label, strconv.QuoteToASCII(jid.Server), label, jid.RawAgent, label, jid.Device, label, jid.Integrator)
}
