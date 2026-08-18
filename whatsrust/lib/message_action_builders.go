package main

import (
	"fmt"
	"strings"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

// reactionRequest keeps the reaction target separate from the destination
// chat. This is required for reactions to status messages.
type reactionRequest struct {
	target, destination, sender types.JID
	id                          types.MessageID
	reaction                    string
}

func newReactionRequest(target, destination, sender types.JID, id types.MessageID, reaction string) (reactionRequest, error) {
	if target.IsEmpty() || destination.IsEmpty() || sender.IsEmpty() || id == "" || reaction == "" {
		return reactionRequest{}, fmt.Errorf("reaction requires target, destination, sender, message ID, and text")
	}
	if target.Server == types.NewsletterServer || destination.Server == types.NewsletterServer {
		return reactionRequest{}, fmt.Errorf("newsletter reactions are not ordinary message reactions")
	}
	return reactionRequest{target: target, destination: destination, sender: sender, id: id, reaction: reaction}, nil
}

func buildOrdinaryReaction(client *whatsmeow.Client, target, sender types.JID, id types.MessageID, reaction string) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	request, err := newReactionRequest(target, target, sender, id, reaction)
	if err != nil {
		return nil, err
	}
	return client.BuildReaction(request.target, request.sender, request.id, request.reaction), nil
}

func buildOrdinaryEdit(client *whatsmeow.Client, chat types.JID, id types.MessageID, replacement string) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	if chat.IsEmpty() || id == "" || strings.TrimSpace(replacement) == "" {
		return nil, fmt.Errorf("edit requires chat, message ID, and replacement text")
	}
	if chat.Server == types.NewsletterServer {
		return nil, fmt.Errorf("newsletter edits are not ordinary message edits")
	}
	return client.BuildEdit(chat, id, &waE2E.Message{Conversation: &replacement}), nil
}

func buildOrdinaryRevoke(client *whatsmeow.Client, chat, sender types.JID, id types.MessageID) (*waE2E.Message, error) {
	if client == nil {
		return nil, fmt.Errorf("WhatsApp client is unavailable")
	}
	if chat.IsEmpty() || sender.IsEmpty() || id == "" {
		return nil, fmt.Errorf("revoke requires chat, sender, and message ID")
	}
	if chat.Server == types.NewsletterServer {
		return nil, fmt.Errorf("newsletter revocations are not ordinary message revocations")
	}
	return client.BuildRevoke(chat, sender, id), nil
}

func parseActionJID(raw string) (types.JID, error) {
	jid, err := types.ParseJID(raw)
	if err != nil || jid.IsEmpty() {
		return types.JID{}, fmt.Errorf("invalid JID")
	}
	return jid, nil
}
