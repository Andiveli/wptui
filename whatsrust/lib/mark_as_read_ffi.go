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

// C_MarkAsRead returns 0 for success, 1 when no connected client exists, 2
// for a retryable bridge failure, and 3 for an invalid permanent request.

//export C_MarkAsRead
func C_MarkAsRead(msgID *C.char, chatJID C.JID, senderJID C.JID) C.int {
	if client == nil || msgID == nil || chatJID == nil || senderJID == nil {
		return 1
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	chat, err := types.ParseJID(C.GoString(chatJID))
	if err != nil || chat.IsEmpty() {
		return 3
	}
	sender, err := types.ParseJID(C.GoString(senderJID))
	if err != nil || sender.IsEmpty() {
		return 3
	}

	return markReadResult(func() error {
		return client.MarkRead(ctx, []types.MessageID{C.GoString(msgID)}, time.Now(), chat, sender)
	})
}

func markReadResult(send func() error) C.int {
	if send == nil {
		return 1
	}
	if send() != nil {
		return 2
	}
	return 0
}
