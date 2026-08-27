package main

import (
	"errors"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
)

func TestBridgeCallersUseLifecycleSnapshots(t *testing.T) {
	for _, file := range []string{
		"contacts.go",
		"communities.go",
		"group_info.go",
		"group_participants.go",
		"chat_settings_ffi.go",
		"dm_resolution.go",
		"mention_names.go",
		"media_downloads.go",
		"profile_picture.go",
		"presence.go",
		"mark_as_read_ffi.go",
		"chat_read_sync_ffi.go",
		"message_callback.go",
		"receipt_events.go",
		"message_action_secret_edit.go",
		"file_message_payload.go",
		"message_event_dispatch.go",
	} {
		source, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(source), "lifecycleState.clientSnapshot()") {
			t.Fatalf("%s does not use the lifecycle client snapshot", file)
		}
	}
}

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

func TestClientLifecycleRegistrationCallbackCanTakeClientSnapshot(t *testing.T) {
	var state clientLifecycleState
	done := make(chan struct{})

	go func() {
		state.registerEventHandlers(func() {
			state.clientSnapshot()
		})
		close(done)
	}()

	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("registration callback deadlocked while taking a client snapshot")
	}
}

func TestClientLifecycleRegistrationIsConcurrentAndOnceOnly(t *testing.T) {
	var state clientLifecycleState
	const callers = 16
	var registrations int
	var registrationsMu sync.Mutex
	started := make(chan struct{})
	release := make(chan struct{})
	register := func() {
		registrationsMu.Lock()
		registrations++
		registrationsMu.Unlock()
		close(started)
		<-release
	}

	var waiters sync.WaitGroup
	waiters.Add(callers)
	for range callers {
		go func() {
			defer waiters.Done()
			state.registerEventHandlers(register)
		}()
	}
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("registration did not start")
	}
	close(release)
	finished := make(chan struct{})
	go func() {
		waiters.Wait()
		close(finished)
	}()
	select {
	case <-finished:
	case <-time.After(time.Second):
		t.Fatal("concurrent registration did not complete")
	}

	registrationsMu.Lock()
	defer registrationsMu.Unlock()
	if registrations != 1 {
		t.Fatalf("registrations = %d, want one", registrations)
	}
}

func TestClientLifecycleRegistrationPanicCanRetry(t *testing.T) {
	var state clientLifecycleState
	func() {
		defer func() {
			if recover() == nil {
				t.Fatal("registration panic was not propagated")
			}
		}()
		state.registerEventHandlers(func() { panic("registration failed") })
	}()

	registrations := 0
	state.registerEventHandlers(func() { registrations++ })
	if registrations != 1 {
		t.Fatalf("retry registrations = %d, want one", registrations)
	}
}

func TestClientLifecycleResetClearsRegistrationAndQR(t *testing.T) {
	qr := make(chan whatsmeow.QRChannelItem)
	previous := client
	t.Cleanup(func() { lifecycleState.publishClient(previous) })
	testClient := &whatsmeow.Client{}
	state := clientLifecycleState{
		qrChan:             qr,
		handlersRegistered: true,
	}
	state.publishClient(testClient)

	state.reset()

	if state.qrChan != nil {
		t.Fatal("reset retained QR channel")
	}
	if state.clientSnapshot() != nil {
		t.Fatal("reset retained client")
	}
	if state.handlersRegistered {
		t.Fatal("reset retained handler registration")
	}
}

func TestClientLifecycleClientSnapshotUsesLifecycleLock(t *testing.T) {
	previous := client
	t.Cleanup(func() { lifecycleState.publishClient(previous) })
	want := &whatsmeow.Client{}
	lifecycleState.publishClient(want)

	if got := lifecycleState.clientSnapshot(); got != want {
		t.Fatalf("client snapshot = %p, want %p", got, want)
	}
}

func TestClientLifecyclePublicationIsSynchronized(t *testing.T) {
	var state clientLifecycleState
	previous := client
	t.Cleanup(func() { lifecycleState.publishClient(previous) })
	first, second := &whatsmeow.Client{}, &whatsmeow.Client{}
	state.publishClient(first)

	state.mu.Lock()
	published := make(chan struct{})
	go func() {
		state.publishClient(second)
		close(published)
	}()
	select {
	case <-published:
		t.Fatal("publication bypassed lifecycle lock")
	default:
	}
	state.mu.Unlock()
	<-published

	if got := state.clientSnapshot(); got != second {
		t.Fatalf("published client = %p, want %p", got, second)
	}
}

func TestClientLifecycleOperationKeepsOneSnapshot(t *testing.T) {
	var state clientLifecycleState
	previous := client
	t.Cleanup(func() { lifecycleState.publishClient(previous) })
	first, second := &whatsmeow.Client{}, &whatsmeow.Client{}
	state.publishClient(first)
	snapshotTaken := make(chan *whatsmeow.Client)
	release := make(chan struct{})

	go func() {
		snapshot := state.clientSnapshot()
		snapshotTaken <- snapshot
		<-release
	}()
	if got := <-snapshotTaken; got != first {
		t.Fatalf("operation snapshot = %p, want %p", got, first)
	}
	state.publishClient(second)
	close(release)
	if got := state.clientSnapshot(); got != second {
		t.Fatalf("published client = %p, want %p", got, second)
	}
}

func TestClientLifecycleReconnectPublishesReplacementSnapshot(t *testing.T) {
	var state clientLifecycleState
	previous := client
	t.Cleanup(func() { lifecycleState.publishClient(previous) })
	first, second := &whatsmeow.Client{}, &whatsmeow.Client{}

	state.publishClient(first)
	if got := state.clientSnapshot(); got != first {
		t.Fatalf("initial client snapshot = %p, want %p", got, first)
	}

	state.reset()
	if got := state.clientSnapshot(); got != nil {
		t.Fatalf("snapshot after reset = %p, want nil", got)
	}

	state.publishClient(second)
	if got := state.clientSnapshot(); got != second {
		t.Fatalf("reconnect client snapshot = %p, want %p", got, second)
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
