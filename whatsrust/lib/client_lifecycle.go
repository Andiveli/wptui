package main

/*
#include <stdlib.h>
#include <stdint.h>
#include <stddef.h>

typedef void (*QrCallback)(const char*, void*);
static void callLifecycleQrCallback(QrCallback cb, const char* code, void* user_data) {
	cb(code, user_data);
}

*/
import "C"

import (
	"context"
	"errors"
	"os"
	"sync"
	"time"
	"unsafe"

	_ "github.com/mattn/go-sqlite3"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"google.golang.org/protobuf/proto"
)

// client remains package-owned until the remaining bridge callers migrate to
// clientSnapshot. Publication and reset are centralized in lifecycleState so
// the migrated callers observe one synchronized lifecycle boundary.
var client *whatsmeow.Client

type logoutReservation struct {
	client     *whatsmeow.Client
	generation uint64
	done       chan struct{}
}

type clientLifecycleState struct {
	mu                   sync.Mutex
	qrChan               <-chan whatsmeow.QRChannelItem
	handlersRegistered   bool
	registrationDone     chan struct{}
	logoutDone           chan struct{}
	logoutGeneration     uint64
	logoutCallbackActive bool
	resetInProgress      bool
}

var lifecycleState clientLifecycleState

// clientSnapshot returns one lifecycle-consistent client pointer for an
// operation. The client remains valid while the caller retains this pointer;
// lifecycle replacement does not mutate the captured client object.
func (state *clientLifecycleState) clientSnapshot() *whatsmeow.Client {
	state.mu.Lock()
	defer state.mu.Unlock()
	return client
}

func (state *clientLifecycleState) publishClient(newClient *whatsmeow.Client) {
	state.mu.Lock()
	defer state.mu.Unlock()
	client = newClient
}

func (state *clientLifecycleState) reset() {
	state.mu.Lock()
	state.resetInProgress = true
	state.logoutGeneration++
	for state.logoutDone != nil {
		done := state.logoutDone
		state.mu.Unlock()
		<-done
		state.mu.Lock()
	}
	for state.registrationDone != nil {
		done := state.registrationDone
		state.mu.Unlock()
		<-done
		state.mu.Lock()
	}
	client = nil
	state.qrChan = nil
	state.handlersRegistered = false
	state.resetInProgress = false
	state.mu.Unlock()
}

func (state *clientLifecycleState) beginLogout() (logoutReservation, bool) {
	state.mu.Lock()
	defer state.mu.Unlock()

	reservation := logoutReservation{client: client}
	if reservation.client == nil || state.logoutDone != nil || state.logoutCallbackActive || state.resetInProgress {
		return reservation, false
	}
	reservation.generation = state.logoutGeneration
	reservation.done = make(chan struct{})
	state.logoutDone = reservation.done
	return reservation, true
}

func (state *clientLifecycleState) completeLogout(done chan struct{}, generation uint64) bool {
	state.mu.Lock()
	defer state.mu.Unlock()

	if state.logoutDone != done {
		return false
	}
	state.logoutDone = nil
	close(done)
	if state.logoutGeneration != generation {
		return false
	}
	state.logoutCallbackActive = true
	return true
}

func (state *clientLifecycleState) finishLogoutCallback() {
	state.mu.Lock()
	state.logoutCallbackActive = false
	state.mu.Unlock()
}

func (state *clientLifecycleState) registerEventHandlers(register func()) {
	state.mu.Lock()
	if state.handlersRegistered {
		state.mu.Unlock()
		return
	}
	if state.registrationDone != nil {
		done := state.registrationDone
		state.mu.Unlock()
		<-done
		state.registerEventHandlers(register)
		return
	}
	done := make(chan struct{})
	state.registrationDone = done
	state.mu.Unlock()

	registered := false
	defer func() {
		state.mu.Lock()
		if state.registrationDone == done {
			if registered {
				state.handlersRegistered = true
			}
			state.registrationDone = nil
			close(done)
		}
		state.mu.Unlock()
	}()

	register()
	registered = true
}

//export C_NewClient
func C_NewClient(dbPath *C.char) {
	lifecycleState.reset()
	clearAuthenticatedPushNameCache()
	rawPresenceProbe.reset(os.Getenv("WPTUI_PRESENCE_DEBUG") == "1")
	requestFullHistorySync()
	goPath := C.GoString(dbPath)
	dbLog := &WrLogger{}
	container, err := sqlstore.New(context.Background(), "sqlite3", "file:"+goPath+"?_foreign_keys=on", dbLog)
	if err != nil {
		panic(err)
	}
	deviceStore, _ := container.GetFirstDevice(context.Background())
	clientLog := &WrLogger{}
	newClient := whatsmeow.NewClient(deviceStore, clientLog)
	configurePresenceSubscriptions(newClient)
	lifecycleState.publishClient(newClient)
}

func requestFullHistorySync() {
	// DeviceProps must be configured before pairing so the registration
	// handshake requests the complete history window.
	store.DeviceProps.RequireFullSync = proto.Bool(true)
	store.DeviceProps.HistorySyncConfig.FullSyncDaysLimit = proto.Uint32(365)
	store.DeviceProps.HistorySyncConfig.FullSyncSizeMbLimit = proto.Uint32(10240)
}

//export C_Connect
func C_Connect(handler C.QrCallback, data unsafe.Pointer) {
	AddEventHandlers()
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot.Store.ID == nil {
		LOG_INFO(
			"Pairing: DeviceProps require_full_sync=%t days_limit=%d size_mb=%d platform=%s",
			store.DeviceProps.GetRequireFullSync(),
			store.DeviceProps.GetHistorySyncConfig().GetFullSyncDaysLimit(),
			store.DeviceProps.GetHistorySyncConfig().GetFullSyncSizeMbLimit(),
			store.DeviceProps.GetPlatformType(),
		)
		lifecycleState.mu.Lock()
		lifecycleState.qrChan, _ = clientSnapshot.GetQRChannel(context.Background())
		qrChannel := lifecycleState.qrChan
		lifecycleState.mu.Unlock()
		if err := clientSnapshot.Connect(); err != nil {
			panic(err)
		}

		for evt := range qrChannel {
			if evt.Event != "code" {
				continue
			}
			code := C.CString(evt.Code)
			C.callLifecycleQrCallback(handler, code, data)
			C.free(unsafe.Pointer(code))
		}
		return
	}
	if err := clientSnapshot.Connect(); err != nil {
		panic(err)
	}
}

//export C_PairPhone
func C_PairPhone(phone *C.char) *C.char {
	clientSnapshot := lifecycleState.clientSnapshot()
	code, err := clientSnapshot.PairPhone(context.Background(), C.GoString(phone), true, whatsmeow.PairClientChrome, "Chrome (Linux)")
	if err != nil {
		panic(err)
	}
	return C.CString(code)
}

// C_FreePairPhoneResult releases the string returned by C_PairPhone. The
// caller owns the result and must invoke this exactly once after copying it.
//
//export C_FreePairPhoneResult
func C_FreePairPhoneResult(result *C.char) {
	if result != nil {
		C.free(unsafe.Pointer(result))
	}
}

//export C_Disconnect
func C_Disconnect() {
	clientSnapshot := lifecycleState.clientSnapshot()
	clientSnapshot.Disconnect()
	clearAuthenticatedPushNameCache()
}

const (
	logoutStatusLoggedOut   uint8 = 0
	logoutStatusNotLoggedIn uint8 = 1
	logoutStatusFailed      uint8 = 2
	logoutStatusLocalOnly   uint8 = 3
)

// logoutStatusAfterRemoteFailure preserves the local-only versus failed
// distinction without coupling the decision to the C ABI or the store call.
func logoutStatusAfterRemoteFailure(localDeleteErr error) uint8 {
	if localDeleteErr != nil {
		return logoutStatusFailed
	}
	return logoutStatusLocalOnly
}

type logoutOperation struct {
	timeout         time.Duration
	remoteLogout    func(context.Context, *whatsmeow.Client) error
	onRemoteFailure func(*whatsmeow.Client, error) uint8
	emit            func(uint8)
	state           *clientLifecycleState
}

func (operation logoutOperation) start(clientSnapshot *whatsmeow.Client) {
	if operation.state != nil {
		reservation, ok := operation.state.beginLogout()
		if !ok {
			return
		}
		operation.startWithReservation(reservation)
		return
	}
	go operation.run(clientSnapshot, nil)
}

func (operation logoutOperation) startWithReservation(reservation logoutReservation) {
	go operation.run(reservation.client, &reservation)
}

func (operation logoutOperation) run(clientSnapshot *whatsmeow.Client, reservation *logoutReservation) {
	ctx, cancel := context.WithTimeout(context.Background(), operation.timeout)
	defer cancel()

	remoteResult := make(chan error, 1)
	go func() {
		remoteResult <- operation.remoteLogout(ctx, clientSnapshot)
	}()

	var err error
	select {
	case err = <-remoteResult:
	case <-ctx.Done():
		err = ctx.Err()
	}
	status := logoutStatusLoggedOut
	switch {
	case err == nil:
		status = logoutStatusLoggedOut
	case errors.Is(err, whatsmeow.ErrNotLoggedIn):
		status = logoutStatusNotLoggedIn
	default:
		status = operation.onRemoteFailure(clientSnapshot, err)
	}
	if reservation != nil {
		if !operation.state.completeLogout(reservation.done, reservation.generation) {
			return
		}
		defer operation.state.finishLogoutCallback()
	}
	operation.emit(status)
}

const logoutStoreDeleteTimeout = 5 * time.Second

func deleteLogoutStore(deleteStore func(context.Context) error) error {
	ctx, cancel := context.WithTimeout(context.Background(), logoutStoreDeleteTimeout)
	defer cancel()
	return deleteStore(ctx)
}

func localLogoutAfterRemoteFailure(clientSnapshot *whatsmeow.Client, err error) uint8 {
	LOG_ERROR("Logout: remote revocation failed, clearing locally only: %v", err)
	clientSnapshot.Disconnect()
	if clientSnapshot.Store == nil {
		return logoutStatusLocalOnly
	}
	localDeleteErr := deleteLogoutStore(clientSnapshot.Store.Delete)
	if localDeleteErr != nil {
		LOG_ERROR("Logout: failed to clear local store: %v", localDeleteErr)
	}
	return logoutStatusAfterRemoteFailure(localDeleteErr)
}

//export C_Logout
func C_Logout() C.uint8_t {
	reservation, started := lifecycleState.beginLogout()
	if reservation.client == nil {
		clearAuthenticatedPushNameCache()
		emitLogoutResult(logoutStatusNotLoggedIn)
		return C.uint8_t(logoutStatusNotLoggedIn)
	}
	if !started {
		return C.uint8_t(logoutStatusLoggedOut)
	}

	logoutOperation{
		timeout:         20 * time.Second,
		remoteLogout:    func(ctx context.Context, client *whatsmeow.Client) error { return client.Logout(ctx) },
		onRemoteFailure: localLogoutAfterRemoteFailure,
		state:           &lifecycleState,
		emit: func(status uint8) {
			clearAuthenticatedPushNameCache()
			emitLogoutResult(status)
		},
	}.startWithReservation(reservation)

	return C.uint8_t(logoutStatusLoggedOut)
}
