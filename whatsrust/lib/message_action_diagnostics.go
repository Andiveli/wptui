package main

import (
	"fmt"

	"go.mau.fi/whatsmeow/types/events"
)

func messageActionStructuralLine(evt *events.Message, branch, reason string) string {
	raw := messageActionProbeFromMessage(evt.RawMessage, "raw")
	normalized := messageActionProbeFromMessage(evt.Message, "message")
	source := messageActionProbe{}
	if evt.SourceWebMsg != nil {
		source = messageActionProbeFromMessage(evt.SourceWebMsg.GetMessage(), "source")
	}
	hasSignal := evt.IsEdit || raw.edited || normalized.edited || source.edited || raw.hasActionProtocol() || normalized.hasActionProtocol() || source.hasActionProtocol()
	if !hasSignal {
		return ""
	}
	selected := raw
	if !selected.hasActionProtocol() && normalized.hasActionProtocol() {
		selected = normalized
	}
	if !selected.hasActionProtocol() && source.hasActionProtocol() {
		selected = source
	}
	if selected.protocol == nil {
		selected = normalized
	}
	if selected.protocol == nil {
		selected = source
	}
	protocolType := "none"
	if selected.protocol != nil {
		protocolType = selected.protocol.GetType().String()
	}
	replacementExists, replacementVariant := selected.replacementVariant()
	return fmt.Sprintf("classifier=structural branch=%s reason=%s is_edit=%t roots=raw:%t,message:%t,source:%t wrappers=edited:%t,ephemeral:%t,view_once:%t,view_once_v2:%t,view_once_v2_extension:%t,device_sent:%t,document_caption:%t,bot_invoke:%t,lottie_sticker:%t,future_proof:%t protocol_path=%s protocol_type=%s protocol_key=%t protocol_key_id=%t replacement_exists=%t replacement_variant=%s action_id=%s target_id=%s chat=%s sender=%s", branch, reason, evt.IsEdit, raw.present, normalized.present, source.present, raw.edited || normalized.edited || source.edited, raw.ephemeral || normalized.ephemeral || source.ephemeral, raw.viewOnce || normalized.viewOnce || source.viewOnce, raw.viewOnceV2 || normalized.viewOnceV2 || source.viewOnceV2, raw.viewOnceV2Extension || normalized.viewOnceV2Extension || source.viewOnceV2Extension, raw.deviceSent || normalized.deviceSent || source.deviceSent, raw.documentWithCaption || normalized.documentWithCaption || source.documentWithCaption, raw.botInvoke || normalized.botInvoke || source.botInvoke, raw.lottieSticker || normalized.lottieSticker || source.lottieSticker, raw.futureProof || normalized.futureProof || source.futureProof, selected.protocolPath, protocolType, selected.protocol.GetKey() != nil, selected.protocol.GetKey().GetID() != "", replacementExists, replacementVariant, messageActionIdentifier(evt.Info.ID), messageActionIdentifier(selected.protocol.GetKey().GetID()), messageActionIdentifier(evt.Info.Chat.String()), messageActionIdentifier(evt.Info.Sender.String()))
}

func messageActionStructuralDiagnostic(evt *events.Message, branch, reason string) {
	if line := messageActionStructuralLine(evt, branch, reason); line != "" {
		messageActionDiagnostic("%s", line)
	}
}
