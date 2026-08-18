package main

import (
	"errors"
	"testing"
)

func TestLogoutStatusAfterRemoteFailure(t *testing.T) {
	for _, testCase := range []struct {
		name           string
		localDeleteErr error
		want           uint8
	}{
		{name: "local store cleared", want: logoutStatusLocalOnly},
		{name: "local store cleanup failed", localDeleteErr: errors.New("disk full"), want: logoutStatusFailed},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			if got := logoutStatusAfterRemoteFailure(testCase.localDeleteErr); got != testCase.want {
				t.Fatalf("status = %d, want %d", got, testCase.want)
			}
		})
	}
}
