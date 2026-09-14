package main

import "go.mau.fi/whatsmeow"

func dispatchConnectionFamily(client *whatsmeow.Client) {
	handleConnected(client.SendPresence, dispatchConnectedEvent, LOG_WARN)
}
