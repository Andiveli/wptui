package main

import waLog "go.mau.fi/whatsmeow/util/log"

// WrLogger adapts whatsmeow logging to the bridge's C-backed logger.
type WrLogger struct{}

func (l *WrLogger) Errorf(msg string, args ...any) {
	LOG_ERROR(msg, args...)
}

func (l *WrLogger) Warnf(msg string, args ...any) {
	LOG_WARN(msg, args...)
}

func (l *WrLogger) Infof(msg string, args ...any) {
	LOG_INFO(msg, args...)
}

func (l *WrLogger) Debugf(msg string, args ...any) {
	LOG_DEBUG(msg, args...)
}

func (l *WrLogger) Sub(module string) waLog.Logger {
	return &WrLogger{}
}
