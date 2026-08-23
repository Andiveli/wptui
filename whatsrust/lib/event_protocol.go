package main

const (
	EventTypeSyncProgress         = 0
	EventTypeAppStateSyncComplete = 1
	EventTypeReceipt              = 2
	EventTypeReaction             = 3
	// Event type 4 is reserved for the removed multiplexed Presence event.
	EventTypeConnected      = 5
	EventTypeMessageAction  = 6
	EventTypeChat           = 7
	EventTypeLogoutResult   = 8
	EventTypeMarkChatAsRead = 9
)
