package main

import (
	"sync/atomic"

	"go.mau.fi/whatsmeow/types/events"
)

var messageActionArrivalOrder uint64

// messageActionEventFromIncomingMessage also supports whatsmeow's normalized edit
// event: UnwrapRaw has already replaced Message with the edited body and Info.ID
// with the target ID. IsEdit is the library's explicit discriminator, so normal
// messages with matching text cannot be mistaken for edits.
func messageActionEventFromIncomingMessage(evt *events.Message) (messageActionEvent, bool) {
	if evt == nil {
		return messageActionEvent{}, false
	}
	rawProbe := messageActionProbeFromMessage(evt.RawMessage, "raw")
	if action, ok, reason := messageActionEventFromProbe(evt.Info, rawProbe); ok {
		if sourceID := evt.SourceWebMsg.GetKey().GetID(); sourceID != "" {
			action.actionID = sourceID
		}
		messageActionStructuralDiagnostic(evt, "raw_classified", "")
		return action, true
	} else if rawProbe.hasActionProtocol() {
		messageActionStructuralDiagnostic(evt, "raw_miss", reason)
		return messageActionEvent{}, false
	}
	if !evt.IsEdit {
		messageActionStructuralDiagnostic(evt, "signal_miss", "protocol_absent")
		return messageActionEvent{}, false
	}
	// Only ParseWebMessage rewrites Info.ID to the target. Live edit envelopes keep
	// Info.ID as the action ID, so never use this fallback without history source data.
	if evt.SourceWebMsg == nil || evt.SourceWebMsg.GetKey().GetID() == "" || evt.Info.ID == "" {
		messageActionStructuralDiagnostic(evt, "normalized_miss", "unproven_target")
		return messageActionEvent{}, false
	}
	replacement, ok := replacementText(evt.Message)
	if !ok {
		messageActionStructuralDiagnostic(evt, "normalized_miss", "missing_replacement")
		return messageActionEvent{}, false
	}
	action := messageActionEvent{
		actionID:        evt.SourceWebMsg.GetKey().GetID(),
		chat:            evt.Info.Chat.String(),
		sender:          evt.Info.Sender.String(),
		targetMessageID: evt.Info.ID,
		replacement:     replacement,
		occurredAt:      evt.Info.Timestamp.Unix(),
		arrivalOrder:    atomic.AddUint64(&messageActionArrivalOrder, 1),
		kind:            messageActionEdit,
	}
	messageActionStructuralDiagnostic(evt, "normalized_classified", "")
	return action, true
}
