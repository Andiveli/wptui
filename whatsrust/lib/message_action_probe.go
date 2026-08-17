package main

import "go.mau.fi/whatsmeow/proto/waE2E"

// messageActionProbe captures wrapper and protocol-edit structure shared by
// action classification and diagnostics.
type messageActionProbe struct {
	present, deviceSent, botInvoke, ephemeral, viewOnce, viewOnceV2, viewOnceV2Extension, lottieSticker, documentWithCaption, edited, futureProof bool
	protocol                                                                                                                                      *waE2E.ProtocolMessage
	secretEncrypted                                                                                                                               *waE2E.SecretEncryptedMessage
	protocolPath                                                                                                                                  string
}

// messageActionProbeFromMessage follows every wrapper that the pinned
// whatsmeow Message.UnwrapRaw follows. Classification and diagnostics share it
// so the reported structure is the structure that was classified.
func messageActionProbeFromMessage(msg *waE2E.Message, path string) messageActionProbe {
	probe := messageActionProbe{present: msg != nil}
	if msg == nil {
		return probe
	}
	if protocol := msg.GetProtocolMessage(); protocol != nil {
		probe.protocol, probe.protocolPath = protocol, path+".protocol"
	}
	if secret := msg.GetSecretEncryptedMessage(); secret != nil {
		probe.secretEncrypted = secret
	}
	merge := func(child messageActionProbe) {
		probe.deviceSent = probe.deviceSent || child.deviceSent
		probe.botInvoke = probe.botInvoke || child.botInvoke
		probe.ephemeral = probe.ephemeral || child.ephemeral
		probe.viewOnce = probe.viewOnce || child.viewOnce
		probe.viewOnceV2 = probe.viewOnceV2 || child.viewOnceV2
		probe.viewOnceV2Extension = probe.viewOnceV2Extension || child.viewOnceV2Extension
		probe.lottieSticker = probe.lottieSticker || child.lottieSticker
		probe.documentWithCaption = probe.documentWithCaption || child.documentWithCaption
		probe.edited = probe.edited || child.edited
		probe.futureProof = probe.futureProof || child.futureProof
		if probe.protocol == nil && child.protocol != nil {
			probe.protocol, probe.protocolPath = child.protocol, child.protocolPath
		}
		if probe.secretEncrypted == nil && child.secretEncrypted != nil {
			probe.secretEncrypted = child.secretEncrypted
		}
	}
	child := func(message *waE2E.Message, name string, mark func()) {
		mark()
		merge(messageActionProbeFromMessage(message, path+"."+name))
	}
	if wrapper := msg.GetDeviceSentMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "device_sent", func() { probe.deviceSent = true })
	}
	if wrapper := msg.GetBotInvokeMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "bot_invoke", func() { probe.botInvoke, probe.futureProof = true, true })
	}
	if wrapper := msg.GetEphemeralMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "ephemeral", func() { probe.ephemeral, probe.futureProof = true, true })
	}
	if wrapper := msg.GetViewOnceMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "view_once", func() { probe.viewOnce, probe.futureProof = true, true })
	}
	if wrapper := msg.GetViewOnceMessageV2(); wrapper != nil {
		child(wrapper.GetMessage(), "view_once_v2", func() { probe.viewOnce, probe.viewOnceV2, probe.futureProof = true, true, true })
	}
	if wrapper := msg.GetViewOnceMessageV2Extension(); wrapper != nil {
		child(wrapper.GetMessage(), "view_once_v2_extension", func() {
			probe.viewOnce, probe.viewOnceV2, probe.viewOnceV2Extension, probe.futureProof = true, true, true, true
		})
	}
	if wrapper := msg.GetLottieStickerMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "lottie_sticker", func() { probe.lottieSticker, probe.futureProof = true, true })
	}
	if wrapper := msg.GetDocumentWithCaptionMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "document_caption", func() { probe.documentWithCaption, probe.futureProof = true, true })
	}
	if wrapper := msg.GetEditedMessage(); wrapper != nil {
		child(wrapper.GetMessage(), "edited", func() { probe.edited, probe.futureProof = true, true })
	}
	return probe
}

func (probe messageActionProbe) hasActionProtocol() bool {
	return probe.protocol != nil && (probe.protocol.GetType() == waE2E.ProtocolMessage_MESSAGE_EDIT || probe.protocol.GetType() == waE2E.ProtocolMessage_REVOKE)
}

func (probe messageActionProbe) replacementVariant() (bool, string) {
	replacement := probe.protocol.GetEditedMessage()
	if replacement == nil {
		return false, "none"
	}
	if replacement.GetConversation() != "" {
		return true, "conversation"
	}
	if replacement.GetExtendedTextMessage().GetText() != "" {
		return true, "extended_text"
	}
	return true, "none"
}
