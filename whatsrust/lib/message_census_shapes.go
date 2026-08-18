package main

import (
	"sort"
	"strings"

	"google.golang.org/protobuf/reflect/protoreflect"

	"go.mau.fi/whatsmeow/proto/waE2E"
)

func messageCensusKinds(msg *waE2E.Message) string {
	if msg == nil {
		return "none"
	}
	var kinds []string
	msg.ProtoReflect().Range(func(field protoreflect.FieldDescriptor, _ protoreflect.Value) bool {
		kinds = append(kinds, safeCensusName(string(field.Name())))
		return true
	})
	sort.Strings(kinds)
	if len(kinds) == 0 {
		return "none"
	}
	return strings.Join(kinds, ",")
}

func messageCensusWrappers(msg *waE2E.Message, path string) string {
	var paths []string
	for msg != nil {
		switch {
		case msg.GetDeviceSentMessage().GetMessage() != nil:
			path += ":device_sent"
			msg = msg.GetDeviceSentMessage().GetMessage()
		case msg.GetBotInvokeMessage().GetMessage() != nil:
			path += ":bot_invoke"
			msg = msg.GetBotInvokeMessage().GetMessage()
		case msg.GetEphemeralMessage().GetMessage() != nil:
			path += ":ephemeral"
			msg = msg.GetEphemeralMessage().GetMessage()
		case msg.GetViewOnceMessage().GetMessage() != nil:
			path += ":view_once"
			msg = msg.GetViewOnceMessage().GetMessage()
		case msg.GetViewOnceMessageV2().GetMessage() != nil:
			path += ":view_once_v2"
			msg = msg.GetViewOnceMessageV2().GetMessage()
		case msg.GetViewOnceMessageV2Extension().GetMessage() != nil:
			path += ":view_once_v2_extension"
			msg = msg.GetViewOnceMessageV2Extension().GetMessage()
		case msg.GetLottieStickerMessage().GetMessage() != nil:
			path += ":lottie_sticker"
			msg = msg.GetLottieStickerMessage().GetMessage()
		case msg.GetDocumentWithCaptionMessage().GetMessage() != nil:
			path += ":document_caption"
			msg = msg.GetDocumentWithCaptionMessage().GetMessage()
		case msg.GetEditedMessage().GetMessage() != nil:
			path += ":edited"
			msg = msg.GetEditedMessage().GetMessage()
		default:
			return path
		}
		paths = append(paths, path)
	}
	return strings.Join(paths, ",")
}
