package main

import (
	"sync"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

func dispatchUndecryptableMessage(evt *events.UndecryptableMessage, dispatchViewOnceUnavailable func(types.MessageInfo, bool)) {
	if !evt.IsUnavailable || evt.UnavailableType != events.UnavailableTypeViewOnce {
		return
	}
	dispatchViewOnceUnavailable(evt.Info, false)
}

type viewOnceUnavailableDispatcher struct {
	mu       sync.Mutex
	seen     map[string]struct{}
	dispatch func(types.MessageInfo, bool)
}

func newViewOnceUnavailableDispatcher(dispatch func(types.MessageInfo, bool)) *viewOnceUnavailableDispatcher {
	return &viewOnceUnavailableDispatcher{seen: make(map[string]struct{}), dispatch: dispatch}
}

func (dispatcher *viewOnceUnavailableDispatcher) dispatchOnce(info types.MessageInfo, isSync bool) {
	if info.ID != "" {
		dispatcher.mu.Lock()
		_, alreadySeen := dispatcher.seen[info.ID]
		if !alreadySeen {
			dispatcher.seen[info.ID] = struct{}{}
		}
		dispatcher.mu.Unlock()
		if alreadySeen {
			return
		}
	}
	dispatcher.dispatch(info, isSync)
}
