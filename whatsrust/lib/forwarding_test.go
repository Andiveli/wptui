package main

import (
	"fmt"
	"testing"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

func TestViewOnceMessagesAreNotCachedForForwarding(t *testing.T) {
	resetForwardedSourcesForTest()
	t.Cleanup(resetForwardedSourcesForTest)
	chat := types.NewJID("chat", types.DefaultUserServer)
	info := types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: chat}, ID: "view-once"}
	message := &waE2E.Message{ViewOnceMessage: &waE2E.FutureProofMessage{Message: &waE2E.Message{ImageMessage: &waE2E.ImageMessage{}}}}

	cacheForwardSource(info, message)

	if len(forwardedSources.entries) != 0 {
		t.Fatal("view-once media must not be cached for forwarding")
	}
}

func TestForwardSourceCacheEvictsAndInvalidatesDeletedSources(t *testing.T) {
	resetForwardedSourcesForTest()
	t.Cleanup(resetForwardedSourcesForTest)
	chat := types.NewJID("source", types.DefaultUserServer)
	sender := types.NewJID("sender", types.DefaultUserServer)
	for index := 0; index <= maxForwardSources; index++ {
		id := types.MessageID(fmt.Sprintf("message-%d", index))
		cacheForwardSource(types.MessageInfo{MessageSource: types.MessageSource{Chat: chat, Sender: sender}, ID: id}, &waE2E.Message{Conversation: stringPointer("payload")})
	}
	first := forwardSourceKey(chat, sender, "message-0")
	last := forwardSourceKey(chat, sender, types.MessageID(fmt.Sprintf("message-%d", maxForwardSources)))
	forwardedSources.mu.Lock()
	_, firstExists := forwardedSources.entries[first]
	_, lastExists := forwardedSources.entries[last]
	entryCount := len(forwardedSources.entries)
	forwardedSources.mu.Unlock()
	if firstExists || !lastExists || entryCount != maxForwardSources {
		t.Fatalf("cache eviction failed: first=%t last=%t count=%d", firstExists, lastExists, entryCount)
	}
	removeForwardSources(chat.String(), "message-"+fmt.Sprint(maxForwardSources))
	forwardedSources.mu.Lock()
	_, lastExists = forwardedSources.entries[last]
	forwardedSources.mu.Unlock()
	if lastExists {
		t.Fatal("deleted source remained forwardable in cache")
	}
}
