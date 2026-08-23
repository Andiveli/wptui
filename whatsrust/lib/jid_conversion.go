package main

/*
#include "callback_log_registration.h"
*/
import "C"

import "go.mau.fi/whatsmeow/types"

// jidToC converts canonical Go JIDs to the stable C bridge representation.
func jidToC(jid types.JID) C.JID {
	return C.CString(jid.ToNonAD().String())
}

// cToJid parses a C bridge JID and preserves the existing panic contract for
// malformed values.
func cToJid(cjid C.JID) types.JID {
	jid, err := types.ParseJID(C.GoString(cjid))
	if err != nil {
		panic(err)
	}
	return jid
}
