package main

import (
	"fmt"
	"strings"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

type statusProtocolContext struct {
	kind string
	info *waE2E.ContextInfo
}

func statusProtocolReactionDiagnostic(info types.MessageInfo, msg *waE2E.Message) (string, bool) {
	reaction := msg.GetReactionMessage()
	if reaction == nil || !isStatusProtocolReaction(info, reaction) {
		return "", false
	}
	key := reaction.GetKey()
	return fmt.Sprintf(
		"status_protocol=reaction chat=%s sender=%s from_me=%t remote_jid=%s participant=%s key_from_me=%t id=%s emoji_codepoints=%s emoji=%s",
		info.Chat.String(), info.Sender.String(), info.IsFromMe,
		key.GetRemoteJID(), key.GetParticipant(), key.GetFromMe(), key.GetID(),
		statusProtocolEmojiCodepoints(reaction.GetText()), statusProtocolEmojiText(reaction.GetText()),
	), true
}

func isStatusProtocolReaction(info types.MessageInfo, reaction *waE2E.ReactionMessage) bool {
	key := reaction.GetKey()
	return info.Chat.String() == "status@broadcast" ||
		key.GetRemoteJID() == "status@broadcast" ||
		key.GetParticipant() == "status@broadcast"
}

func statusProtocolEmojiCodepoints(text string) string {
	if text == "" {
		return "none"
	}
	codepoints := make([]string, 0, len(text))
	for _, r := range text {
		codepoints = append(codepoints, fmt.Sprintf("U+%04X", r))
	}
	return strings.Join(codepoints, ",")
}

func statusProtocolEmojiText(text string) string {
	if text == "" {
		return "<empty>"
	}
	return text
}

func statusProtocolQuotedMessageKind(message *waE2E.Message) string {
	switch {
	case message == nil:
		return "none"
	case message.GetConversation() != "":
		return "text"
	case message.GetExtendedTextMessage() != nil:
		return "extended_text"
	case message.GetImageMessage() != nil:
		return "image"
	case message.GetVideoMessage() != nil:
		return "video"
	case message.GetAudioMessage() != nil:
		return "audio"
	case message.GetDocumentMessage() != nil:
		return "document"
	case message.GetStickerMessage() != nil:
		return "sticker"
	default:
		return "other"
	}
}

func statusProtocolContextDiagnostics(info types.MessageInfo, msg *waE2E.Message) []string {
	contexts := []statusProtocolContext{
		{kind: "extended_text", info: msg.GetExtendedTextMessage().GetContextInfo()},
		{kind: "image", info: msg.GetImageMessage().GetContextInfo()},
		{kind: "video", info: msg.GetVideoMessage().GetContextInfo()},
		{kind: "audio", info: msg.GetAudioMessage().GetContextInfo()},
		{kind: "document", info: msg.GetDocumentMessage().GetContextInfo()},
		{kind: "sticker", info: msg.GetStickerMessage().GetContextInfo()},
	}
	lines := make([]string, 0, len(contexts))
	for _, context := range contexts {
		if context.info == nil || !isStatusProtocolContext(context.info) {
			continue
		}
		quotedMessage := context.info.GetQuotedMessage()
		lines = append(lines, fmt.Sprintf(
			"status_protocol=context chat=%s sender=%s from_me=%t content=%s stanza_id=%s participant=%s remote_jid=%s poster_status_id=%s quoted_message_present=%t quoted_message_kind=%s status_source_type_present=%t status_source_type=%d status_attribution_type_present=%t status_attribution_type=%d is_group_status_present=%t is_group_status=%t",
			info.Chat.String(), info.Sender.String(), info.IsFromMe, context.kind,
			context.info.GetStanzaID(), context.info.GetParticipant(), context.info.GetRemoteJID(), context.info.GetPosterStatusID(), quotedMessage != nil, statusProtocolQuotedMessageKind(quotedMessage),
			context.info.StatusSourceType != nil, context.info.GetStatusSourceType(),
			context.info.StatusAttributionType != nil, context.info.GetStatusAttributionType(),
			context.info.IsGroupStatus != nil, context.info.GetIsGroupStatus(),
		))
	}
	return lines
}
