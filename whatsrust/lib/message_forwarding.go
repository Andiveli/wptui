package main

import (
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func forwardingSourcePayload(info types.MessageInfo, message *waE2E.Message, unavailable bool) []byte {
	if unavailable {
		return nil
	}

	cacheForwardSource(info, message)
	rawSource, err := marshalForwardSource(message)
	if err != nil {
		LOG_WARN("forward source serialization failed: %v", err)
		return nil
	}
	return rawSource
}
