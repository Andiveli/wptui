package main

import (
	"errors"
	"testing"

	"go.mau.fi/whatsmeow"
)

func TestClientLifecycleRegistrationIsIdempotent(t *testing.T) {
	var state clientLifecycleState
	registrations := 0
	register := func() { registrations++ }

	state.registerEventHandlers(register)
	state.registerEventHandlers(register)

	if registrations != 1 {
		t.Fatalf("registrations = %d, want one", registrations)
	}
}

func TestClientLifecycleResetClearsRegistrationAndQR(t *testing.T) {
	qr := make(chan whatsmeow.QRChannelItem)
	state := clientLifecycleState{
		qrChan:             qr,
		handlersRegistered: true,
	}

	state.reset()

	if state.qrChan != nil {
		t.Fatal("reset retained QR channel")
	}
	if state.handlersRegistered {
		t.Fatal("reset retained handler registration")
	}
}

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
