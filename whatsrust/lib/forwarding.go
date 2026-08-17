package main

import (
	"slices"
	"sync"

	"google.golang.org/protobuf/proto"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

const maxForwardSources = 1000

func forwardSourceKey(chat, sender types.JID, id types.MessageID) string {
	return chat.String() + "\x00" + sender.String() + "\x00" + string(id)
}

type forwardSource struct {
	info    types.MessageInfo
	message *waE2E.Message
}

type forwardSourceCache struct {
	mu      sync.Mutex
	entries map[string]forwardSource
	order   []string
}

var forwardedSources = forwardSourceCache{entries: make(map[string]forwardSource)}

func resetForwardedSourcesForTest() {
	forwardedSources.mu.Lock()
	defer forwardedSources.mu.Unlock()
	forwardedSources.entries = make(map[string]forwardSource)
	forwardedSources.order = nil
}

func removeForwardSources(chat, id string) {
	forwardedSources.mu.Lock()
	defer forwardedSources.mu.Unlock()
	for key, source := range forwardedSources.entries {
		if source.info.Chat.String() == chat && string(source.info.ID) == id {
			delete(forwardedSources.entries, key)
		}
	}
	forwardedSources.order = slices.DeleteFunc(forwardedSources.order, func(key string) bool {
		_, exists := forwardedSources.entries[key]
		return !exists
	})
}

func cacheForwardSource(info types.MessageInfo, message *waE2E.Message) {
	if message == nil || info.ID == "" || containsViewOnceWrapper(message) {
		return
	}
	key := forwardSourceKey(info.Chat, info.Sender, info.ID)
	forwardedSources.mu.Lock()
	defer forwardedSources.mu.Unlock()
	if _, exists := forwardedSources.entries[key]; !exists {
		forwardedSources.order = append(forwardedSources.order, key)
		if len(forwardedSources.order) > maxForwardSources {
			delete(forwardedSources.entries, forwardedSources.order[0])
			forwardedSources.order = forwardedSources.order[1:]
		}
	}
	forwardedSources.entries[key] = forwardSource{info: info, message: proto.Clone(message).(*waE2E.Message)}
}
