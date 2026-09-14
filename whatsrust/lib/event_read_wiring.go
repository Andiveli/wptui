package main

import "go.mau.fi/whatsmeow/types/events"

func dispatchReadFamily(evt *events.MarkChatAsRead) {
	dispatchMarkChatAsReadEvent(evt)
}
