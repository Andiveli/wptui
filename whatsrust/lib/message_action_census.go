package main

import (
	"fmt"
	"sync"
)

const messageActionCensusLimit = 100

type messageActionCensus struct {
	mu      sync.Mutex
	nextSeq uint64
	entries []string
}

var eventCensus messageActionCensus

func messageActionCensusAppend(entry string) {
	eventCensus.mu.Lock()
	eventCensus.nextSeq++
	entry = fmt.Sprintf("census=event seq=%d %s", eventCensus.nextSeq, entry)
	if len(eventCensus.entries) == messageActionCensusLimit {
		eventCensus.entries = eventCensus.entries[1:]
	}
	eventCensus.entries = append(eventCensus.entries, entry)
	eventCensus.mu.Unlock()
	messageActionDiagnostic("%s", entry)
}
