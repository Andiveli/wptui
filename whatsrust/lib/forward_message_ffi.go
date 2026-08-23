package main

/*
#include <stdint.h>
#include <stddef.h>
#include "callback_log_registration.h"

typedef struct {
	uint32_t succeeded;
	uint32_t failed;
	uint8_t failure;
} ForwardResult;
*/
import "C"

import "unsafe"

//export C_ForwardMessage
func C_ForwardMessage(sourceID *C.char, sourceChat C.JID, sourceSender C.JID, sourceIsFromMe C.bool, destinations **C.char, destinationCount C.size_t, forwardSource *C.uint8_t, forwardSourceLen C.size_t) C.ForwardResult {
	if sourceID == nil || sourceChat == nil || sourceSender == nil || destinations == nil || destinationCount == 0 {
		return C.ForwardResult{}
	}
	rawDestinations := unsafe.Slice(destinations, int(destinationCount))
	destinationStrings := make([]string, 0, len(rawDestinations))
	for _, destination := range rawDestinations {
		if destination == nil {
			return C.ForwardResult{failed: C.uint32_t(destinationCount)}
		}
		destinationStrings = append(destinationStrings, C.GoString(destination))
	}
	report := forwardMessages(
		C.GoString(sourceID),
		C.GoString(sourceChat),
		C.GoString(sourceSender),
		bool(sourceIsFromMe),
		destinationStrings,
		forwardingSourceBytes(forwardSource, forwardSourceLen),
	).cResult()
	return report
}
