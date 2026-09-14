package main

import (
	"context"
	"errors"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

type sendMessageRequest func(*whatsmeow.Client, context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error)

var requestSendMessage sendMessageRequest = func(client *whatsmeow.Client, ctx context.Context, jid types.JID, message *waE2E.Message) (whatsmeow.SendResponse, error) {
	return client.SendMessage(ctx, jid, message)
}

const optimisticTextSendTimeout = 5 * time.Second

const (
	outboundSendSent uint8 = iota
	outboundSendInvalidRequest
	outboundSendClientUnavailable
	outboundSendPreparationFailed
	outboundSendTimedOut
	outboundSendCancelled
	outboundSendTransportFailed
)

func optimisticTextSendContext(parent context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	return context.WithTimeout(parent, timeout)
}

func sendOutboundRequest(request textSendRequest) uint8 {
	ctx, cancel := optimisticTextSendContext(context.Background(), optimisticTextSendTimeout)
	defer cancel()
	return sendOutboundRequestWithContext(ctx, request)
}

func sendOutboundRequestWithContext(sendContext context.Context, request textSendRequest) uint8 {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil || clientSnapshot.Store.ID == nil {
		LOG_WARN("message send rejected: client is unavailable")
		return outboundSendClientUnavailable
	}
	contextInfo := &waE2E.ContextInfo{}
	if request.quote != nil {
		contextInfo = quotedContextInfo(request.quote.stanzaID, request.quote.participant, request.quote.remoteJID)
		contextInfo.QuotedMessage = request.quote.content
	}
	message, err := buildOutboundMessage(sendContext, request, contextInfo, clientSnapshot.Upload)
	if err != nil {
		LOG_WARN("message preparation failed: %v", err)
		return contextStatus(err, outboundSendPreparationFailed)
	}

	sendResponse, err := requestSendMessage(clientSnapshot, sendContext, request.chat, message)
	if err != nil {
		LOG_WARN("message send failed: %v", err)
		return contextStatus(err, outboundSendTransportFailed)
	}

	messageInfo := types.MessageInfo{
		MessageSource: types.MessageSource{Chat: request.chat, Sender: *clientSnapshot.Store.ID, IsFromMe: true},
		ID:            sendResponse.ID,
		Timestamp:     sendResponse.Timestamp,
	}
	LOG_INFO("Message sent: %s %s", messageInfo.ID, messageInfo.Chat)
	emitOutboundCallback(request, messageInfo, message)
	return outboundSendSent
}

func contextStatus(err error, fallback uint8) uint8 {
	switch {
	case errors.Is(err, context.DeadlineExceeded):
		return outboundSendTimedOut
	case errors.Is(err, context.Canceled):
		return outboundSendCancelled
	default:
		return fallback
	}
}
