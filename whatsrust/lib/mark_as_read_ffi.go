package main

/*
#include "callback_log_registration.h"
*/
import "C"

import (
	"context"
	"time"

	"go.mau.fi/whatsmeow/types"
)

type markAsReadFunc func(context.Context, []types.MessageID, time.Time, types.JID, types.JID, ...types.ReceiptType) error

func markMessageAsRead(msgID string, chat, sender types.JID, markRead markAsReadFunc) {
	_ = markRead(context.Background(), []types.MessageID{types.MessageID(msgID)}, time.Now(), chat, sender)
}

//export C_MarkAsRead
func C_MarkAsRead(msgID *C.char, chatJID C.JID, senderJID C.JID) {
	markMessageAsRead(C.GoString(msgID), cToJid(chatJID), cToJid(senderJID), client.MarkRead)
}
