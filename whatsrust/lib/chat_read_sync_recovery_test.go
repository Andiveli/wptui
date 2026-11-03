package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"reflect"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
	"google.golang.org/protobuf/proto"
)

type readRecoveryClient struct {
	send  func(context.Context, appstate.PatchInfo) error
	fetch func(context.Context, appstate.WAPatchName, bool, bool) error
}

func (client readRecoveryClient) SendAppState(ctx context.Context, patch appstate.PatchInfo) error {
	return client.send(ctx, patch)
}

func (client readRecoveryClient) FetchAppState(ctx context.Context, name appstate.WAPatchName, full, onlyIfNotSynced bool) error {
	return client.fetch(ctx, name, full, onlyIfNotSynced)
}

func TestChatReadStateRecoverySequenceAndBridgeResult(t *testing.T) {
	conflict := fmt.Errorf("%w: %w", whatsmeow.ErrAppStateUpdate, appstate.ErrMismatchingLTHash)
	failure := errors.New("private recovery error")
	for _, read := range []bool{false, true} {
		for _, tt := range []struct {
			name, category         string
			initial, fetch, resend error
			cancelled, empty       bool
			want                   []string
		}{
			{name: "ordinary success", category: "none", want: []string{"send"}},
			{name: "repair", initial: conflict, category: "none", want: []string{"send", "fetch", "send"}},
			{name: "repair empty chat", initial: conflict, empty: true, category: "none", want: []string{"send", "fetch", "send"}},
			{name: "repair failure", initial: conflict, fetch: failure, category: "other", want: []string{"send", "fetch"}},
			{name: "resend failure", initial: conflict, resend: failure, category: "other", want: []string{"send", "fetch", "send"}},
			{name: "no recovery loop", initial: conflict, resend: conflict, category: "appstate_cause_lt_hash", want: []string{"send", "fetch", "send"}},
			{name: "post success fetch", initial: fmt.Errorf("failed to fetch app state after sending update: %w", appstate.ErrMismatchingLTHash), category: "resync_after_send", want: []string{"send"}},
			{name: "bare hash error", initial: appstate.ErrMismatchingLTHash, category: "other", want: []string{"send"}},
			{name: "other integrity error", initial: fmt.Errorf("%w: %w", whatsmeow.ErrAppStateUpdate, appstate.ErrMismatchingContentMAC), category: "appstate_cause_content_mac", want: []string{"send"}},
			{name: "ordinary failure", initial: failure, category: "other", want: []string{"send"}},
			{name: "cancelled parent", initial: conflict, cancelled: true, category: "appstate_cause_lt_hash", want: []string{"send"}},
		} {
			t.Run(fmt.Sprintf("%s/read=%v", tt.name, read), func(t *testing.T) {
				ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
				defer cancel()
				if tt.cancelled {
					cancel()
				}
				deadline, _ := ctx.Deadline()
				messageID, timestamp := "latest", int64(42)
				if tt.empty {
					messageID, timestamp = "", 0
				}
				var calls []string
				var firstValue proto.Message
				var firstIndex []string
				cacheReady := tt.initial == nil
				client := readRecoveryClient{
					send: func(active context.Context, patch appstate.PatchInfo) error {
						calls = append(calls, "send")
						if got, ok := active.Deadline(); !ok || !got.Equal(deadline) {
							t.Fatal("send changed parent deadline")
						}
						if patch.Type != appstate.WAPatchRegularLow || len(patch.Mutations) != 1 {
							t.Fatal("send changed patch type/count")
						}
						mutation := patch.Mutations[0]
						action := mutation.Value.GetMarkChatAsReadAction()
						if action.GetRead() != read || action.GetMessageRange().GetLastMessageTimestamp() != timestamp {
							t.Fatal("read state/range changed")
						}
						if tt.empty && len(action.GetMessageRange().GetMessages()) != 0 {
							t.Fatal("empty chat acquired fabricated identity")
						}
						if firstValue == nil {
							firstValue = proto.Clone(mutation.Value)
							firstIndex = append([]string(nil), mutation.Index...)
						} else if !proto.Equal(firstValue, mutation.Value) || !reflect.DeepEqual(firstIndex, mutation.Index) {
							t.Fatal("resend changed original mutation or identity")
						}
						if !cacheReady {
							return tt.initial
						}
						return tt.resend
					},
					fetch: func(active context.Context, name appstate.WAPatchName, full, onlyIfNotSynced bool) error {
						calls = append(calls, "fetch")
						if name != appstate.WAPatchRegularLow || !full || onlyIfNotSynced {
							t.Fatal("incorrect cache rebuild scope/flags")
						}
						if got, ok := active.Deadline(); !ok || !got.Equal(deadline) {
							t.Fatal("fetch changed parent deadline")
						}
						if active.Err() != nil {
							t.Fatal("fetch used cancelled context")
						}
						if tt.fetch == nil {
							cacheReady = true
						}
						return tt.fetch
					},
				}
				var state clientLifecycleState
				identity := &whatsmeow.Client{}
				state.publishClient(identity)
				var report bytes.Buffer
				request := func() error {
					return sendChatReadStateSync(ctx, "alice@s.whatsapp.net", messageID, read, timestamp, false, "",
						func(ctx context.Context, patch appstate.PatchInfo) error {
							return sendChatReadStatePatch(ctx, patch, client,
								func(ctx context.Context, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
									return state.withChatReadRecovery(ctx, identity, name, operation)
								})
						})
				}
				status := markReadResultWithTrace(request, &report)
				wantStatus := 2
				if tt.category == "none" {
					wantStatus = 0
				}
				wantReport := fmt.Sprintf("stage=bridge status=%d category=%s\n", wantStatus, tt.category)
				if int(status) != wantStatus || report.String() != wantReport || !reflect.DeepEqual(calls, tt.want) {
					t.Fatalf("status=%d report=%q calls=%v; want status=%d report=%q calls=%v", status, report.String(), calls, wantStatus, wantReport, tt.want)
				}
				if tt.name == "repair failure" {
					firstValue = nil
					report.Reset()
					status = markReadResultWithTrace(request, &report)
					if status != 2 || report.String() != "stage=bridge status=2 category=appstate_cause_lt_hash\n" || !reflect.DeepEqual(calls, []string{"send", "fetch", "send"}) {
						t.Fatalf("queued failure repeated repair: status=%d report=%q calls=%v", status, report.String(), calls)
					}
				}
			})
		}
	}
	if got := C_MarkChatReadStateSync(nil, nil, false, 0, false, nil); got != 1 {
		t.Fatalf("unavailable bridge status=%d", got)
	}
}

// Stage evidence must retain discarded errors without changing legacy results.
func TestChatReadRecoveryStageObservationsPreserveResults(t *testing.T) {
	conflict := fmt.Errorf("%w: %w", whatsmeow.ErrAppStateUpdate, appstate.ErrMismatchingLTHash)
	fetchFailure := errors.New("private fetch payload")
	resendFailure := fmt.Errorf("private resend identity: %w", whatsmeow.ErrAppStateUpdate)
	type observation struct {
		stage string
		err   error
	}
	for _, tt := range []struct {
		name, category         string
		initial, fetch, resend error
		exhausted              bool
		wantErr                error
		wantCalls              []string
		wantEvents             []observation
	}{
		{name: "fetch failure", category: "other", initial: conflict, fetch: fetchFailure, wantErr: fetchFailure,
			wantCalls: []string{"send", "fetch"}, wantEvents: []observation{{"initial_send", conflict}, {"recovery_fetch", fetchFailure}}},
		{name: "resend failure", category: "appstate_rejected", initial: conflict, resend: resendFailure, wantErr: resendFailure,
			wantCalls: []string{"send", "fetch", "send"}, wantEvents: []observation{{"initial_send", conflict}, {"recovery_fetch", nil}, {"resend", resendFailure}}},
		{name: "resend success", category: "none", initial: conflict,
			wantCalls: []string{"send", "fetch", "send"}, wantEvents: []observation{{"initial_send", conflict}, {"recovery_fetch", nil}, {"resend", nil}}},
		{name: "simple rejection", category: "appstate_rejected", initial: whatsmeow.ErrAppStateUpdate, wantErr: whatsmeow.ErrAppStateUpdate,
			wantCalls: []string{"send"}, wantEvents: []observation{{"initial_send", whatsmeow.ErrAppStateUpdate}}},
		{name: "exhausted budget", category: "appstate_cause_lt_hash", initial: conflict, fetch: fetchFailure, exhausted: true, wantErr: conflict,
			wantCalls: []string{"send"}, wantEvents: []observation{{"initial_send", conflict}}},
	} {
		for _, enabled := range []bool{false, true} {
			for _, read := range []bool{false, true} {
				t.Run(fmt.Sprintf("%s/observe=%v/read=%v", tt.name, enabled, read), func(t *testing.T) {
					ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
					defer cancel()
					var state clientLifecycleState
					identity := &whatsmeow.Client{}
					state.publishClient(identity)
					var calls []string
					var events []observation
					client := readRecoveryClient{
						send: func(context.Context, appstate.PatchInfo) error {
							calls = append(calls, "send")
							if len(calls) == 1 {
								return tt.initial
							}
							return tt.resend
						},
						fetch: func(context.Context, appstate.WAPatchName, bool, bool) error {
							calls = append(calls, "fetch")
							return tt.fetch
						},
					}
					patch, err := buildChatReadStateSyncPatch("alice@s.whatsapp.net", "latest", read, 42, false, "")
					if err != nil {
						t.Fatal(err)
					}
					scope := func(ctx context.Context, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
						return state.withChatReadRecovery(ctx, identity, name, operation)
					}
					if tt.exhausted {
						if got := sendChatReadStatePatch(ctx, patch, client, scope); got != fetchFailure || !reflect.DeepEqual(calls, []string{"send", "fetch"}) {
							t.Fatal("budget setup changed recovery result or calls")
						}
						calls = nil
					}
					var observers []chatReadStageObserver
					var report bytes.Buffer
					var output io.Writer
					if enabled {
						observers = append(observers, func(stage string, err error) { events = append(events, observation{stage, err}) })
						output = &report
					}
					var result error
					status := markReadResultWithTrace(func() error {
						result = sendChatReadStatePatch(ctx, patch, client, scope, observers...)
						return result
					}, output)
					wantStatus := 0
					if tt.wantErr != nil {
						wantStatus = 2
					}
					wantReport := ""
					var wantEvents []observation
					if enabled {
						wantReport = fmt.Sprintf("stage=bridge status=%d category=%s\n", wantStatus, tt.category)
						wantEvents = tt.wantEvents
					}
					if result != tt.wantErr || int(status) != wantStatus || !reflect.DeepEqual(calls, tt.wantCalls) || report.String() != wantReport {
						t.Fatal("observer changed returned error, bridge status/report, or send/fetch order")
					}
					if len(events) != len(wantEvents) {
						t.Fatalf("stage observations missing or changed: got %d events, want %d", len(events), len(wantEvents))
					}
					for i, event := range events {
						if event.stage != wantEvents[i].stage || event.err != wantEvents[i].err {
							t.Fatalf("stage observation %d changed order or original error", i)
						}
					}
				})
			}
		}
	}
}

func TestChatReadRecoveryBudgetFollowsClientAndCollection(t *testing.T) {
	var state clientLifecycleState
	first, second := &whatsmeow.Client{}, &whatsmeow.Client{}
	state.publishClient(first)
	calls := 0
	claim := func(client *whatsmeow.Client, name appstate.WAPatchName, want bool) {
		t.Helper()
		before := calls
		claimed, err := state.withChatReadRecovery(context.Background(), client, name, func(context.Context) error { calls++; return errors.New("repair failed") })
		if claimed != want || (want && (err == nil || calls != before+1)) || (!want && (err != nil || calls != before)) {
			t.Fatalf("claim=%v err=%v calls=%d before=%d want=%v", claimed, err, calls, before, want)
		}
	}
	claim(first, appstate.WAPatchRegularLow, true)
	claim(first, appstate.WAPatchRegularLow, false)
	claim(first, appstate.WAPatchRegularHigh, true)
	state.publishClient(first)
	claim(first, appstate.WAPatchRegularLow, false)
	state.publishClient(second)
	claim(first, appstate.WAPatchRegularLow, false)
	claim(second, appstate.WAPatchRegularLow, true)
	state.retire(func(*whatsmeow.Client) {})
	claim(second, appstate.WAPatchRegularLow, false)
	third := &whatsmeow.Client{EmitAppStateEventsOnFullSync: true}
	state.publishClient(third)
	claim(third, appstate.WAPatchRegularLow, false)
}

func TestChatReadRecoveryLifecycleCancelsWithoutJoining(t *testing.T) {
	for _, transition := range []string{"publish", "reset", "retire", "logout"} {
		t.Run(transition, func(t *testing.T) {
			var state clientLifecycleState
			identity := &whatsmeow.Client{}
			state.publishClient(identity)
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			started, cancelled, release := make(chan struct{}), make(chan struct{}), make(chan struct{})
			defer func() {
				select {
				case <-release:
				default:
					close(release)
				}
			}()
			repairDone, transitioned := make(chan struct{}), make(chan struct{})
			var repairErr error
			sends := 0
			client := readRecoveryClient{
				send: func(context.Context, appstate.PatchInfo) error {
					sends++
					return fmt.Errorf("%w: %w", whatsmeow.ErrAppStateUpdate, appstate.ErrMismatchingLTHash)
				},
				fetch: func(active context.Context, _ appstate.WAPatchName, _, _ bool) error {
					close(started)
					<-active.Done()
					close(cancelled)
					<-release // Model a fetch that does not promptly return on cancellation.
					return nil
				},
			}
			go func() {
				defer close(repairDone)
				repairErr = sendChatReadStatePatch(ctx, appstate.PatchInfo{Type: appstate.WAPatchRegularLow}, client,
					func(ctx context.Context, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
						return state.withChatReadRecovery(ctx, identity, name, operation)
					})
			}()
			select {
			case <-started:
			case <-repairDone:
				t.Fatalf("repair never started: %v", repairErr)
			case <-ctx.Done():
				t.Fatal("repair did not start before deadline")
			}
			go func() {
				defer close(transitioned)
				switch transition {
				case "publish":
					state.publishClient(&whatsmeow.Client{})
				case "reset":
					state.reset()
				case "retire":
					state.retire(func(*whatsmeow.Client) {})
				case "logout":
					_, _ = state.beginLogout()
				}
			}()
			select {
			case <-cancelled:
			case <-time.After(250 * time.Millisecond):
				t.Error("lifecycle did not cancel repair")
			}
			transitionFinished := false
			select {
			case <-transitioned:
				transitionFinished = true
			case <-time.After(250 * time.Millisecond):
				t.Error("lifecycle joined blocked repair")
			}
			// Register a new client's lease before allowing the old lease to clean up.
			newStarted, newDone := make(chan struct{}), make(chan struct{})
			newCtx, newCancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer newCancel()
			if transition == "publish" && transitionFinished {
				current := state.clientSnapshot()
				go func() {
					defer close(newDone)
					_, _ = state.withChatReadRecovery(newCtx, current, appstate.WAPatchRegularLow, func(active context.Context) error {
						close(newStarted)
						<-active.Done()
						return active.Err()
					})
				}()
				select {
				case <-newStarted:
				case <-newDone:
					t.Fatal("replacement repair was denied")
				case <-newCtx.Done():
					t.Fatal("replacement repair did not start")
				}
			}
			cancel()
			close(release)
			<-repairDone
			<-transitioned
			if sends != 1 || !errors.Is(repairErr, context.Canceled) {
				t.Errorf("cancelled repair resent: sends=%d err=%v", sends, repairErr)
			}
			if transition == "publish" && transitionFinished {
				state.retire(func(*whatsmeow.Client) {})
				select {
				case <-newDone:
				case <-time.After(250 * time.Millisecond):
					t.Error("old cleanup erased replacement lease")
				}
				newCancel()
				<-newDone
			}
		})
	}
}
