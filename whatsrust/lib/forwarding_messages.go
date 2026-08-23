package main

import (
	"fmt"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"
)

func forwardingContext(existing *waE2E.ContextInfo, sourceIsFromMe bool) *waE2E.ContextInfo {
	context := &waE2E.ContextInfo{}
	if existing != nil {
		context = proto.Clone(existing).(*waE2E.ContextInfo)
	}
	if sourceIsFromMe {
		context.IsForwarded = proto.Bool(false)
		context.ForwardingScore = nil
		return context
	}
	forwarded := true
	score := context.GetForwardingScore() + 1
	if score > 5 {
		score = 5
	}
	context.ForwardingScore = &score
	context.IsForwarded = &forwarded
	return context
}

func sourceOwnedByCurrentUser(sourceIsFromMe bool, sourceSender, self types.JID) bool {
	return sourceIsFromMe || sourceSender.ToNonAD() == self.ToNonAD()
}

func marshalForwardSource(message *waE2E.Message) ([]byte, error) {
	if message == nil || containsViewOnceWrapper(message) {
		return nil, nil
	}
	return proto.Marshal(message)
}

func forwardSourceFromBytes(raw []byte) (*waE2E.Message, uint8) {
	if len(raw) == 0 {
		return nil, forwardFailureSourceUnavailable
	}
	message := &waE2E.Message{}
	if err := proto.Unmarshal(raw, message); err != nil {
		return nil, forwardFailureSourceUnavailable
	}
	return message, forwardFailureNone
}

func prepareForwardMessage(source *waE2E.Message, sourceIsFromMe bool) (*waE2E.Message, error) {
	if source == nil {
		return nil, fmt.Errorf("forward source is unavailable")
	}
	forwarded, ok := proto.Clone(source).(*waE2E.Message)
	if !ok {
		return nil, fmt.Errorf("forward source cannot be cloned")
	}
	switch {
	case forwarded.Conversation != nil:
		forwarded.ExtendedTextMessage = &waE2E.ExtendedTextMessage{Text: forwarded.Conversation, ContextInfo: forwardingContext(nil, sourceIsFromMe)}
		forwarded.Conversation = nil
	case forwarded.ExtendedTextMessage != nil:
		forwarded.ExtendedTextMessage.ContextInfo = forwardingContext(forwarded.ExtendedTextMessage.ContextInfo, sourceIsFromMe)
	case forwarded.ImageMessage != nil:
		forwarded.ImageMessage.ContextInfo = forwardingContext(forwarded.ImageMessage.ContextInfo, sourceIsFromMe)
	case forwarded.VideoMessage != nil:
		forwarded.VideoMessage.ContextInfo = forwardingContext(forwarded.VideoMessage.ContextInfo, sourceIsFromMe)
	case forwarded.AudioMessage != nil:
		forwarded.AudioMessage.ContextInfo = forwardingContext(forwarded.AudioMessage.ContextInfo, sourceIsFromMe)
	case forwarded.DocumentMessage != nil:
		forwarded.DocumentMessage.ContextInfo = forwardingContext(forwarded.DocumentMessage.ContextInfo, sourceIsFromMe)
	case forwarded.StickerMessage != nil:
		forwarded.StickerMessage.ContextInfo = forwardingContext(forwarded.StickerMessage.ContextInfo, sourceIsFromMe)
	default:
		return nil, fmt.Errorf("message content is not forwardable")
	}
	return forwarded, nil
}
