package main

import (
	"context"
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

func TestLogoutOperationIsNonBlockingAndEmitsOneTerminalResult(t *testing.T) {
	for _, testCase := range []struct {
		name       string
		remoteErr  error
		wantStatus uint8
	}{
		{name: "success", wantStatus: logoutStatusLoggedOut},
		{name: "error", remoteErr: errors.New("remote unavailable"), wantStatus: logoutStatusLocalOnly},
		{name: "timeout", remoteErr: context.DeadlineExceeded, wantStatus: logoutStatusLocalOnly},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			started := make(chan struct{})
			finished := make(chan struct{})
			emitted := make(chan uint8, 2)
			operation := logoutOperation{
				timeout: 20 * time.Millisecond,
				remoteLogout: func(ctx context.Context, _ *whatsmeow.Client) error {
					close(started)
					if testCase.remoteErr == context.DeadlineExceeded {
						<-ctx.Done()
						return ctx.Err()
					}
					return testCase.remoteErr
				},
				onRemoteFailure: func(_ *whatsmeow.Client, _ error) uint8 {
					return logoutStatusLocalOnly
				},
				emit: func(status uint8) {
					emitted <- status
					close(finished)
				},
			}
			operation.start(&whatsmeow.Client{})
			select {
			case <-started:
			case <-time.After(time.Second):
				t.Fatal("logout operation did not start")
			}
			select {
			case <-finished:
			case <-time.After(time.Second):
				t.Fatal("logout operation did not finish")
			}
			if got := <-emitted; got != testCase.wantStatus {
				t.Fatalf("terminal status = %d, want %d", got, testCase.wantStatus)
			}
			select {
			case extra := <-emitted:
				t.Fatalf("received duplicate terminal status %d", extra)
			case <-time.After(50 * time.Millisecond):
			}
		})
	}
}
func TestLogoutStoreDeleteUsesBoundedContext(t *testing.T) {
	called := make(chan context.Context, 1)
	deleteErr := errors.New("delete stopped")
	got := deleteLogoutStore(func(ctx context.Context) error {
		called <- ctx
		return deleteErr
	})
	if !errors.Is(got, deleteErr) {
		t.Fatalf("delete error = %v, want %v", got, deleteErr)
	}
	select {
	case ctx := <-called:
		if _, ok := ctx.Deadline(); !ok {
			t.Fatal("logout store delete context has no deadline")
		}
	case <-time.After(time.Second):
		t.Fatal("logout store delete was not called")
	}
}
func TestClientLifecycleAllowsOnlyOneLogoutInFlight(t *testing.T) {
	var state clientLifecycleState
	previous := client
	t.Cleanup(func() { state.publishClient(previous) })
	firstClient := &whatsmeow.Client{}
	state.publishClient(firstClient)
	first, ok := state.beginLogout()
	if !ok {
		t.Fatal("first logout did not start")
	}
	second, ok := state.beginLogout()
	if ok {
		t.Fatal("second logout started while the first was in flight")
	}
	if second.client != firstClient {
		t.Fatalf("second logout client = %p, want %p", second.client, firstClient)
	}
	if !state.completeLogout(first.done, first.generation) {
		t.Fatal("first logout completion was rejected")
	}
	third, ok := state.beginLogout()
	if ok {
		t.Fatal("logout started while the terminal callback was in flight")
	}
	if third.client != firstClient {
		t.Fatalf("terminal-callback logout client = %p, want %p", third.client, firstClient)
	}
	state.finishLogoutCallback()
}
func waitForLifecycleCondition(t *testing.T, state *clientLifecycleState, condition func() bool, message string) {
	t.Helper()
	deadline := time.After(time.Second)
	for {
		state.mu.Lock()
		ready := condition()
		state.mu.Unlock()
		if ready {
			return
		}
		select {
		case <-deadline:
			t.Fatal(message)
		default:
			time.Sleep(time.Millisecond)
		}
	}
}
func TestClientLifecycleResetWaitsForLogoutAndSuppressesStaleResult(t *testing.T) {
	var state clientLifecycleState
	previous := client
	t.Cleanup(func() { state.publishClient(previous) })
	clientSnapshot := &whatsmeow.Client{}
	state.publishClient(clientSnapshot)
	started := make(chan struct{})
	release := make(chan struct{})
	emitted := make(chan struct{}, 1)
	operation := logoutOperation{
		timeout: time.Second,
		state:   &state,
		emit:    func(uint8) { emitted <- struct{}{} },
		remoteLogout: func(context.Context, *whatsmeow.Client) error {
			close(started)
			<-release
			return nil
		},
	}
	operation.start(clientSnapshot)
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("logout operation did not start")
	}
	state.registrationDone = make(chan struct{})
	resetDone := make(chan struct{})
	go func() {
		state.reset()
		close(resetDone)
	}()
	waitForLifecycleCondition(t, &state, func() bool { return state.logoutGeneration != 0 && state.logoutDone != nil }, "reset did not wait for logout")
	close(release)
	waitForLifecycleCondition(t, &state, func() bool { return state.logoutDone == nil }, "logout did not complete")
	admitted, admissionStarted := state.beginLogout()
	if admissionStarted || admitted.client != clientSnapshot {
		t.Fatalf("reset admitted logout for client %p, started=%t", admitted.client, admissionStarted)
	}
	close(state.registrationDone)
	state.mu.Lock()
	state.registrationDone = nil
	state.mu.Unlock()
	select {
	case <-resetDone:
	case <-time.After(time.Second):
		t.Fatal("reset did not complete after logout")
	}
	select {
	case <-emitted:
		t.Fatal("stale logout result was emitted after reset")
	default:
	}
}
func TestClientLifecycleLogoutCompletionClearsBeforeSynchronousCallback(t *testing.T) {
	var state clientLifecycleState
	previous := client
	t.Cleanup(func() { state.publishClient(previous) })
	clientSnapshot := &whatsmeow.Client{}
	state.publishClient(clientSnapshot)
	started := make(chan struct{})
	callbackDone := make(chan struct{})
	operation := logoutOperation{
		timeout: time.Second,
		state:   &state,
		remoteLogout: func(context.Context, *whatsmeow.Client) error {
			close(started)
			return nil
		},
		emit: func(uint8) {
			state.reset()
			close(callbackDone)
		},
	}
	logout, ok := state.beginLogout()
	if !ok {
		t.Fatal("logout did not start")
	}
	operation.startWithReservation(logout)
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("logout operation did not start")
	}
	select {
	case <-callbackDone:
	case <-time.After(time.Second):
		t.Fatal("synchronous logout callback deadlocked reset")
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
