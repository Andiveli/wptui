package main

/*
#include <stdlib.h>
#include "callback_log_registration.h"

static void callPresenceWiringHandler(PresenceHandler hdl, JID from, bool unavailable, int64_t lastSeen) {
	hdl.callback(from, unavailable, lastSeen, hdl.user_data);
}
*/
import "C"

import (
	"unsafe"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types/events"
)

func dispatchPresenceCallback(from string, unavailable bool, lastSeen int64) {
	cFrom := C.CString(from)
	defer C.free(unsafe.Pointer(cFrom))
	C.callPresenceWiringHandler(presenceHandler, cFrom, C.bool(unavailable), C.int64_t(lastSeen))
}

func dispatchPresenceFamily(evt *events.Presence, client *whatsmeow.Client) {
	dispatchPresenceEvent(evt, client.Store.LIDs.GetPNForLID, rawPresenceProbe.record, rawPresenceProbe.update, dispatchPresenceCallback)
}
