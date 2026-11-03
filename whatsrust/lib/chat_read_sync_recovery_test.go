package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
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
				status := markReadResultWithTrace(func() error {
					return sendChatReadStateSync(ctx, "alice@s.whatsapp.net", messageID, read, timestamp, false, "",
						func(ctx context.Context, patch appstate.PatchInfo) error {
							return sendChatReadStatePatch(ctx, patch, client,
								func(ctx context.Context, name appstate.WAPatchName, operation func(context.Context) error) (bool, error) {
									return state.withChatReadRecovery(ctx, identity, name, operation)
								})
						})
				}, &report)
				wantStatus := 2
				if tt.category == "none" {
					wantStatus = 0
				}
				wantReport := fmt.Sprintf("stage=bridge status=%d category=%s\n", wantStatus, tt.category)
				if int(status) != wantStatus || report.String() != wantReport || !reflect.DeepEqual(calls, tt.want) {
					t.Fatalf("status=%d report=%q calls=%v; want status=%d report=%q calls=%v", status, report.String(), calls, wantStatus, wantReport, tt.want)
				}
			})
		}
	}
	if got := C_MarkChatReadStateSync(nil, nil, false, 0, false, nil); got != 1 {
		t.Fatalf("unavailable bridge status=%d", got)
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
	second.EmitAppStateEventsOnFullSync = true
	state.publishClient(second)
	claim(second, appstate.WAPatchRegularLow, false)
}

func TestChatReadRecoveryLifecycleCancelsAndJoinsActiveRepair(t *testing.T) {
	for _, transition := range []string{"publish", "reset", "retire", "logout"} {
		t.Run(transition, func(t *testing.T) {
			var state clientLifecycleState
			client := &whatsmeow.Client{}
			state.publishClient(client)
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			started, cancelled, release := make(chan struct{}), make(chan struct{}), make(chan struct{})
			repairDone, transitionStarted, transitioned := make(chan struct{}), make(chan struct{}), make(chan struct{})
			go func() {
				defer close(repairDone)
				_, _ = state.withChatReadRecovery(ctx, client, appstate.WAPatchRegularLow, func(active context.Context) error {
					close(started)
					<-active.Done()
					close(cancelled)
					<-release
					return active.Err()
				})
			}()
			<-started
			go func() {
				defer close(transitioned)
				close(transitionStarted)
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
			<-transitionStarted
			select {
			case <-cancelled:
			case <-time.After(250 * time.Millisecond):
				t.Error("lifecycle did not cancel repair")
			}
			select {
			case <-transitioned:
				t.Error("lifecycle returned before active repair finished")
			default:
			}
			cancel()
			close(release)
			<-repairDone
			<-transitioned
		})
	}
}
