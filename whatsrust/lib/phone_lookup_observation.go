package main

import "context"

// lookupPhoneObserved keeps local inspection separate from the accepted recipient.
// The observation is populated only for a rejected, matched response.
func lookupPhoneObserved(ctx context.Context, phone string, query phoneLookupQuery) (phoneLookupResult, string) {
	return lookupPhone(ctx, phone, query), ""
}
