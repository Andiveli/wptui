package main

import "os"

func messageActionDiagnostic(msg string, args ...any) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	LOG_WARN("MESSAGE_ACTION_DIAG "+msg, args...)
}

func emitStatusProtocolDiagnostic(emit func(string, ...any), entry string) {
	if os.Getenv("WPTUI_MESSAGE_ACTION_DEBUG") != "1" {
		return
	}
	emit("%s", entry)
}
