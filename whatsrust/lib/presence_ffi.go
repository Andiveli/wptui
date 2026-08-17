package main

/*
#include <stdbool.h>
#include <stdint.h>
#include "callback_log_registration.h"

static void callPresenceTestHandler(PresenceHandler hdl, JID from, bool unavailable, int64_t lastSeen) {
	hdl.callback(from, unavailable, lastSeen, hdl.user_data);
}
*/
import "C"

import "sync"

//export C_TestEmitPresenceEvent
func C_TestEmitPresenceEvent(from *C.char, unavailable C.bool, lastSeen C.int64_t) {
	C.callPresenceTestHandler(presenceHandler, from, unavailable, lastSeen)
}

//export C_TestEmitPresenceEventsConcurrently
func C_TestEmitPresenceEventsConcurrently(from *C.char, count C.uint32_t) {
	var wait sync.WaitGroup
	for index := uint32(0); index < uint32(count); index++ {
		wait.Add(1)
		go func(lastSeen uint32) {
			defer wait.Done()
			C.callPresenceTestHandler(presenceHandler, from, C.bool(false), C.int64_t(lastSeen))
		}(index)
	}
	wait.Wait()
}
