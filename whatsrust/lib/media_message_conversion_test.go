package main

import (
	"context"
	"strings"
	"testing"
)

func TestBuildFileMessageReportsReadFailure(t *testing.T) {
	_, err := buildFileMessage(context.Background(), FileTypeImage, "missing.bin", nil, nil, nil)
	if err == nil || !strings.Contains(err.Error(), "read file") {
		t.Fatalf("error = %v, want file-read error", err)
	}
}
