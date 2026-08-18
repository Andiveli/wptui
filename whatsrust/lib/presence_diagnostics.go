package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"unsafe"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
)

const maxRawPresenceDiagnosticEntries = 50

type rawPresenceDiagnosticEntry struct {
	sequence        uint64
	server          string
	unavailable     bool
	lastSeenPresent bool
	normalized      string
	normalization   string
	dispatch        string
}

type rawPresenceDiagnostics struct {
	mu      sync.Mutex
	enabled atomic.Bool
	total   uint64
	entries []rawPresenceDiagnosticEntry
}

var rawPresenceProbe rawPresenceDiagnostics

func classifyPresenceServer(server string) string {
	switch server {
	case types.DefaultUserServer:
		return "s.whatsapp.net"
	case types.HiddenUserServer:
		return "lid"
	default:
		return "other"
	}
}

func (diagnostics *rawPresenceDiagnostics) reset(enabled bool) {
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	diagnostics.total = 0
	diagnostics.entries = nil
	diagnostics.enabled.Store(enabled)
}

func (diagnostics *rawPresenceDiagnostics) record(event *events.Presence) uint64 {
	if !diagnostics.enabled.Load() {
		return 0
	}
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	if !diagnostics.enabled.Load() {
		return 0
	}
	diagnostics.total++
	if len(diagnostics.entries) == maxRawPresenceDiagnosticEntries {
		copy(diagnostics.entries, diagnostics.entries[1:])
		diagnostics.entries = diagnostics.entries[:maxRawPresenceDiagnosticEntries-1]
	}
	diagnostics.entries = append(diagnostics.entries, rawPresenceDiagnosticEntry{
		sequence:        diagnostics.total,
		server:          classifyPresenceServer(event.From.Server),
		unavailable:     event.Unavailable,
		lastSeenPresent: !event.LastSeen.IsZero(),
	})
	return diagnostics.total
}

func (diagnostics *rawPresenceDiagnostics) update(sequence uint64, normalized, normalization, dispatch string) {
	if !diagnostics.enabled.Load() {
		return
	}
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	for index := range diagnostics.entries {
		if diagnostics.entries[index].sequence == sequence {
			diagnostics.entries[index].normalized = normalized
			diagnostics.entries[index].normalization = normalization
			diagnostics.entries[index].dispatch = dispatch
			return
		}
	}
}

func (diagnostics *rawPresenceDiagnostics) drain() string {
	if !diagnostics.enabled.Load() {
		return ""
	}
	diagnostics.mu.Lock()
	defer diagnostics.mu.Unlock()
	if !diagnostics.enabled.Load() {
		return ""
	}
	var report strings.Builder
	fmt.Fprintf(&report, "raw presence events received: %d\n", diagnostics.total)
	firstSequence := diagnostics.total - uint64(len(diagnostics.entries)) + 1
	for index, entry := range diagnostics.entries {
		fmt.Fprintf(&report, "%d. server=%s, unavailable=%t, last_seen_present=%t, normalized=%s, normalization=%s, dispatch=%s\n", firstSequence+uint64(index), entry.server, entry.unavailable, entry.lastSeenPresent, entry.normalized, entry.normalization, entry.dispatch)
	}
	diagnostics.total = 0
	diagnostics.entries = nil
	return report.String()
}

//export C_DrainRawPresenceDiagnostics
func C_DrainRawPresenceDiagnostics() *C.char {
	report := rawPresenceProbe.drain()
	if report == "" {
		return nil
	}
	return C.CString(report)
}

//export C_FreeRawPresenceDiagnostics
func C_FreeRawPresenceDiagnostics(report *C.char) {
	C.free(unsafe.Pointer(report))
}
