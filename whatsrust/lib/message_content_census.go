package main

import (
	"strings"

	waE2E "go.mau.fi/whatsmeow/proto/waE2E"
)

func msgContentTypes(msg *waE2E.Message) string {
	if msg == nil {
		return "nil"
	}
	parts := make([]string, 0, 8)
	if msg.Conversation != nil {
		parts = append(parts, "conversation")
	}
	if msg.ExtendedTextMessage != nil {
		parts = append(parts, "extended_text")
	}
	if msg.ProtocolMessage != nil {
		pm := msg.ProtocolMessage
		s := "protocol"
		if pm.GetEditedMessage() != nil {
			s += "+edited"
		}
		parts = append(parts, s)
	}
	if msg.EditedMessage != nil {
		s := "edited"
		if em := msg.EditedMessage.GetMessage(); em != nil {
			s += "+inner"
			if em.Conversation != nil {
				s += "_conv"
			}
			if em.ExtendedTextMessage != nil {
				s += "_ext"
			}
		}
		parts = append(parts, s)
	}
	if msg.ReactionMessage != nil {
		parts = append(parts, "reaction")
	}
	if len(parts) == 0 {
		return "empty"
	}
	return strings.Join(parts, ",")
}
