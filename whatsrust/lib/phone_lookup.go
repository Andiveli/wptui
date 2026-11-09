package main

/*
#include <stdint.h>
#include <stdlib.h>

typedef struct {
	uint8_t status;
	char* jid;
} PhoneLookupResult;
*/
import "C"

import (
	"context"
	"errors"
	"time"
	"unsafe"

	"go.mau.fi/whatsmeow/types"
)

const (
	phoneLookupFound uint8 = iota
	phoneLookupNotRegistered
	phoneLookupInvalidNumber
	phoneLookupUnavailable
	phoneLookupFailed
	phoneLookupTimedOut
	phoneLookupEmptyResponse
	phoneLookupResponseMismatch
	phoneLookupQueryMismatch
	phoneLookupLidOnly
	phoneLookupDifferentPhoneNumber
)

type phoneLookupResult struct {
	status uint8
	jid    string
}

type phoneLookupQuery func(context.Context, []string) ([]types.IsOnWhatsAppResponse, error)

// A positive server response for exactly the requested PN is the only
// authority to open an unsaved conversation. Keep the PN as the chat key:
// an unknown LID has no display name and would disappear from the chat list.
func lookupPhone(ctx context.Context, phone string, query phoneLookupQuery) phoneLookupResult {
	if len(phone) < 8 || len(phone) > 16 || phone[0] != '+' {
		return phoneLookupResult{status: phoneLookupInvalidNumber}
	}
	for _, digit := range phone[1:] {
		if digit < '0' || digit > '9' {
			return phoneLookupResult{status: phoneLookupInvalidNumber}
		}
	}
	if query == nil {
		return phoneLookupResult{status: phoneLookupUnavailable}
	}
	responses, err := query(ctx, []string{phone})
	if err != nil {
		if errors.Is(err, context.DeadlineExceeded) {
			return phoneLookupResult{status: phoneLookupTimedOut}
		}
		return phoneLookupResult{status: phoneLookupFailed}
	}
	if len(responses) == 0 {
		return phoneLookupResult{status: phoneLookupEmptyResponse}
	}
	requested := types.NewJID(phone[1:], types.DefaultUserServer)
	for _, response := range responses {
		if response.Query != phone && response.Query != phone[1:] {
			continue
		}
		if !response.IsIn {
			return phoneLookupResult{status: phoneLookupNotRegistered}
		}
		if matchesRequestedPhoneIdentity(response.PhoneNumber, requested) || matchesRequestedPhoneIdentity(response.JID, requested) {
			return phoneLookupResult{status: phoneLookupFound, jid: requested.String()}
		}
		// These categories describe response shape only, not an alternate recipient.
		if response.JID.Server == types.HiddenUserServer && response.JID.User != "" && response.PhoneNumber == (types.JID{}) {
			return phoneLookupResult{status: phoneLookupLidOnly}
		}
		if (response.JID.Server == types.DefaultUserServer && response.JID.User != "") ||
			(response.PhoneNumber.Server == types.DefaultUserServer && response.PhoneNumber.User != "") {
			return phoneLookupResult{status: phoneLookupDifferentPhoneNumber}
		}
		return phoneLookupResult{status: phoneLookupResponseMismatch}
	}
	return phoneLookupResult{status: phoneLookupQueryMismatch}
}

// matchesRequestedPhoneIdentity recognizes only the SDK's legacy user namespace
// as equivalent to the canonical PN namespace. Digits and integrator stay exact.
func matchesRequestedPhoneIdentity(identity, requested types.JID) bool {
	identity = identity.ToNonAD()
	if identity.Server == types.LegacyUserServer {
		identity.Server = types.DefaultUserServer
	}
	return identity == requested
}

//export C_LookupPhone
func C_LookupPhone(phone *C.char) C.PhoneLookupResult {
	if phone == nil {
		return C.PhoneLookupResult{status: C.uint8_t(phoneLookupInvalidNumber)}
	}
	client := lifecycleState.clientSnapshot()
	if client == nil || client.Store == nil || client.Store.ID == nil {
		return C.PhoneLookupResult{status: C.uint8_t(phoneLookupUnavailable)}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	result := lookupPhone(ctx, C.GoString(phone), client.IsOnWhatsApp)
	if result.status != phoneLookupFound {
		return C.PhoneLookupResult{status: C.uint8_t(result.status)}
	}
	return C.PhoneLookupResult{status: C.uint8_t(result.status), jid: C.CString(result.jid)}
}

//export C_FreeLookupPhoneResult
func C_FreeLookupPhoneResult(result C.PhoneLookupResult) {
	C.free(unsafe.Pointer(result.jid))
}
