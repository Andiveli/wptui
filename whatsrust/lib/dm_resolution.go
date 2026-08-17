package main

/*
#include <stdlib.h>
#include "callback_log_registration.h"
*/
import "C"

import (
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
)

type dmChatIDResolver func(types.JID) string

func resolveDMChatID(client *whatsmeow.Client, jidText string, resolve dmChatIDResolver) (string, bool) {
	if client == nil {
		return "", false
	}
	jid, err := types.ParseJID(jidText)
	if err != nil || jid.IsEmpty() {
		return "", false
	}
	return resolve(jid), true
}

// Resolve a user JID (typically a group participant) to its canonical
// direct-conversation id. Direct chats are stored under the phone number
// (PN); a group participant may be a LID, so we map LID→PN when known so the
// private reply opens the real chat instead of an empty LID-keyed thread.
// Non-LID personal JIDs pass through unchanged. Returns NULL when the
// client is not ready or the JID cannot be parsed.
//
//export C_ResolveDmChatId
func C_ResolveDmChatId(cjid C.JID) *C.char {
	value, ok := resolveDMChatID(client, C.GoString(cjid), func(jid types.JID) string {
		return GetChatId(client, &jid, nil)
	})
	if !ok {
		return nil
	}
	return C.CString(value)
}

//export C_FreeResolveDmChatId
func C_FreeResolveDmChatId(value *C.char) {
	C.free(unsafe.Pointer(value))
}
