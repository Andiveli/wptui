package main

/*
#include <stdint.h>

typedef struct {
	uint32_t succeeded;
	uint32_t failed;
	uint8_t failure;
} ForwardResult;
*/
import "C"

import (
	"context"
	"unsafe"

	"go.mau.fi/whatsmeow/types"
)

type forwardingReport struct {
	succeeded uint32
	failed    uint32
	failure   uint8
}

func (report forwardingReport) cResult() C.ForwardResult {
	return C.ForwardResult{
		succeeded: C.uint32_t(report.succeeded),
		failed:    C.uint32_t(report.failed),
		failure:   C.uint8_t(report.failure),
	}
}

func forwardMessages(sourceID, sourceChat, sourceSender string, sourceIsFromMe bool, destinations []string, rawSource []byte) forwardingReport {
	request, err := newForwardRequest(sourceChat, sourceSender, sourceID, destinations)
	if err != nil {
		return forwardingReport{failed: uint32(len(destinations)), failure: forwardFailureInvalidSource}
	}
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil || clientSnapshot.Store.ID == nil {
		return forwardingReport{failed: uint32(len(request.destinations)), failure: forwardFailureSendFailed}
	}
	sourceMessage, failure := forwardSourceFromBytes(rawSource)
	if failure != forwardFailureNone {
		return forwardingReport{failed: uint32(len(request.destinations)), failure: failure}
	}
	sourceOwned := sourceOwnedByCurrentUser(sourceIsFromMe, request.sourceSender, *clientSnapshot.Store.ID)
	report := forwardingReport{}
	for _, destination := range request.destinations {
		message, err := prepareForwardMessage(sourceMessage, sourceOwned)
		if err != nil {
			report.failed++
			continue
		}
		response, err := clientSnapshot.SendMessage(context.Background(), destination, message)
		if err != nil {
			LOG_WARN("forward send failed: %v", err)
			report.failed++
			continue
		}
		report.succeeded++
		HandleMessage(types.MessageInfo{MessageSource: types.MessageSource{Chat: destination, Sender: *clientSnapshot.Store.ID, IsFromMe: true}, ID: response.ID, Timestamp: response.Timestamp}, message, false)
	}
	return report
}

func forwardingSourceBytes(pointer *C.uint8_t, length C.size_t) []byte {
	if pointer == nil || length == 0 {
		return nil
	}
	return unsafe.Slice((*byte)(unsafe.Pointer(pointer)), int(length))
}
