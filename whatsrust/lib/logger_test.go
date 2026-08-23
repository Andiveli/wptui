package main

import (
	"os"
	"strings"
	"testing"
)

func TestWhatsmeowLoggerAdapterStaysDedicated(t *testing.T) {
	source, err := os.ReadFile("logger.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, expected := range []string{
		"type WrLogger struct{}",
		"func (l *WrLogger) Errorf",
		"func (l *WrLogger) Warnf",
		"func (l *WrLogger) Infof",
		"func (l *WrLogger) Debugf",
		"func (l *WrLogger) Sub(module string) waLog.Logger",
	} {
		if !strings.Contains(string(source), expected) {
			t.Fatalf("logger adapter missing %q", expected)
		}
	}
}
