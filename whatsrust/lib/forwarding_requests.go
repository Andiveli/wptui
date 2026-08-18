package main

import (
	"fmt"

	"go.mau.fi/whatsmeow/types"
)

// forwardRequest is the validated, typed input for the forwarding orchestration.
type forwardRequest struct {
	sourceChat, sourceSender types.JID
	sourceID                 types.MessageID
	destinations             []types.JID
}

func forwardableJID(jid types.JID) bool {
	return !jid.IsEmpty() && jid.Server != types.BroadcastServer && jid.Server != types.NewsletterServer
}

// newForwardRequest owns the forwarding boundary: it parses all JIDs and
// rejects destinations and sources that cannot participate in ordinary chats.
func newForwardRequest(sourceChat, sourceSender, sourceID string, destinations []string) (forwardRequest, error) {
	chat, err := parseActionJID(sourceChat)
	if err != nil || !forwardableJID(chat) {
		return forwardRequest{}, fmt.Errorf("forward source chat is invalid")
	}
	sender, err := parseActionJID(sourceSender)
	if err != nil || sender.IsEmpty() || sourceID == "" {
		return forwardRequest{}, fmt.Errorf("forward requires source sender and message ID")
	}
	if len(destinations) == 0 {
		return forwardRequest{}, fmt.Errorf("forward requires at least one destination")
	}
	request := forwardRequest{sourceChat: chat, sourceSender: sender, sourceID: types.MessageID(sourceID)}
	for _, raw := range destinations {
		destination, err := parseActionJID(raw)
		if err != nil || !forwardableJID(destination) {
			return forwardRequest{}, fmt.Errorf("forward destination is invalid")
		}
		request.destinations = append(request.destinations, destination)
	}
	return request, nil
}
