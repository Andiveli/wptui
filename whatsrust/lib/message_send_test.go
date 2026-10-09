package main

import (
	"context"
	"errors"
	"os"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

func TestSendStatusRequestTargetsStatusBroadcastAndMapsSupportedPayloads(t *testing.T) {
	imagePath := t.TempDir() + "/status-image.png"
	videoPath := t.TempDir() + "/status-video.mp4"
	for _, path := range []string{imagePath, videoPath} {
		if err := os.WriteFile(path, []byte("media"), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	caption := "status caption"
	cases := []struct {
		name      string
		request   statusSendRequest
		mediaType *whatsmeow.MediaType
	}{
		{
			name:    "text",
			request: statusSendRequest{messageType: MessageTypeText, text: "status text"},
		},
		{
			name:      "image",
			request:   statusSendRequest{messageType: MessageTypeFile, fileKind: FileTypeImage, filePath: imagePath, caption: &caption},
			mediaType: ptr(whatsmeow.MediaImage),
		},
		{
			name:      "video",
			request:   statusSendRequest{messageType: MessageTypeFile, fileKind: FileTypeVideo, filePath: videoPath, caption: &caption},
			mediaType: ptr(whatsmeow.MediaVideo),
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			var gotDestination types.JID
			var gotMessage *waE2E.Message
			result := sendStatusRequest(
				context.Background(),
				tc.request,
				nil,
				func(_ context.Context, _ []byte, mediaType whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
					if tc.mediaType == nil || mediaType != *tc.mediaType {
						t.Fatalf("upload media type = %q, want %v", mediaType, tc.mediaType)
					}
					return whatsmeow.UploadResponse{URL: "https://example.test/status", DirectPath: "/status"}, nil
				},
				func(_ *whatsmeow.Client, _ context.Context, destination types.JID, message *waE2E.Message) (whatsmeow.SendResponse, error) {
					gotDestination = destination
					gotMessage = message
					return whatsmeow.SendResponse{ID: "status-send"}, nil
				},
			)
			if result != statusSendResultSent {
				t.Fatalf("status result = %d, want sent", result)
			}
			if gotDestination != types.StatusBroadcastJID {
				t.Fatalf("status destination = %s, want %s", gotDestination, types.StatusBroadcastJID)
			}
			switch tc.name {
			case "text":
				if gotMessage.GetExtendedTextMessage().GetText() != "status text" || gotMessage.GetExtendedTextMessage().ContextInfo != nil {
					t.Fatalf("text status payload = %#v", gotMessage.GetExtendedTextMessage())
				}
			case "image":
				if gotMessage.GetImageMessage().GetCaption() != caption || gotMessage.GetImageMessage().ContextInfo != nil {
					t.Fatalf("image status payload = %#v", gotMessage.GetImageMessage())
				}
			case "video":
				if gotMessage.GetVideoMessage().GetCaption() != caption || gotMessage.GetVideoMessage().ContextInfo != nil {
					t.Fatalf("video status payload = %#v", gotMessage.GetVideoMessage())
				}
			}
		})
	}
}

func TestSendStatusRequestReportsUploadAndSendFailures(t *testing.T) {
	mediaPath := t.TempDir() + "/status-image.png"
	if err := os.WriteFile(mediaPath, []byte("image"), 0o600); err != nil {
		t.Fatal(err)
	}
	request := statusSendRequest{messageType: MessageTypeFile, fileKind: FileTypeImage, filePath: mediaPath}
	if result := sendStatusRequest(context.Background(), request, nil,
		func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
			return whatsmeow.UploadResponse{}, errors.New("upload failed")
		},
		func(*whatsmeow.Client, context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error) {
			t.Fatal("send must not run after upload failure")
			return whatsmeow.SendResponse{}, nil
		},
	); result != statusSendResultMediaPreparationFailed {
		t.Fatalf("upload result = %d, want media preparation failure", result)
	}

	if result := sendStatusRequest(context.Background(), statusSendRequest{messageType: MessageTypeText, text: "status"}, nil, nil,
		func(*whatsmeow.Client, context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error) {
			return whatsmeow.SendResponse{}, errors.New("send failed")
		},
	); result != statusSendResultSendFailed {
		t.Fatalf("send result = %d, want send failure", result)
	}
}

func TestStatusSendCorrelatesCanonicalIDWithoutChangingLegacyResult(t *testing.T) {
	self := types.NewJID("self", types.DefaultUserServer)
	client := &whatsmeow.Client{Store: &store.Device{ID: &self}}
	request := statusSendRequest{messageType: MessageTypeText, text: "status"}
	var callbacks []types.MessageInfo
	onSent := func(localID uint64, info types.MessageInfo, message *waE2E.Message) {
		if localID != 42 || message.GetExtendedTextMessage().GetText() != "status" {
			t.Errorf("callback local ID = %d, message = %v", localID, message)
		}
		callbacks = append(callbacks, info)
	}
	response := whatsmeow.SendResponse{ID: "canonical-status", Timestamp: time.Now()}
	result := sendStatusRequestWithLocalID(context.Background(), request, client, nil,
		func(*whatsmeow.Client, context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error) {
			return response, nil
		}, 42, onSent)
	if result != statusSendResultSent || len(callbacks) != 1 || callbacks[0].ID != response.ID || callbacks[0].Chat != types.StatusBroadcastJID || !callbacks[0].IsFromMe || callbacks[0].Sender != self {
		t.Fatalf("result = %d, canonical callbacks = %+v", result, callbacks)
	}
	for _, media := range []struct {
		name string
		kind uint8
	}{
		{name: "image", kind: FileTypeImage},
		{name: "video", kind: FileTypeVideo},
	} {
		t.Run(media.name, func(t *testing.T) {
			path := t.TempDir() + "/status-file"
			if err := os.WriteFile(path, []byte("media"), 0o600); err != nil {
				t.Fatal(err)
			}
			callbacks = nil
			request := statusSendRequest{messageType: MessageTypeFile, fileKind: media.kind, filePath: path}
			result := sendStatusRequestWithLocalID(context.Background(), request, client,
				func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
					return whatsmeow.UploadResponse{URL: "https://example.test/status", DirectPath: "/status"}, nil
				},
				func(*whatsmeow.Client, context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error) {
					return response, nil
				}, 42,
				func(id uint64, info types.MessageInfo, message *waE2E.Message) {
					if id != 42 || info.ID != response.ID || (media.kind == FileTypeImage && message.GetImageMessage() == nil) || (media.kind == FileTypeVideo && message.GetVideoMessage() == nil) {
						t.Errorf("unexpected media callback: id=%d info=%+v", id, info)
					}
					callbacks = append(callbacks, info)
				})
			if result != statusSendResultSent || len(callbacks) != 1 {
				t.Fatalf("result = %d, callbacks = %+v", result, callbacks)
			}
		})
	}
	for _, tc := range []struct {
		name     string
		localID  uint64
		response whatsmeow.SendResponse
		err      error
	}{
		{name: "legacy send", localID: 0, response: response},
		{name: "transport error", localID: 42, err: errors.New("unknown outcome")},
		{name: "missing canonical ID", localID: 42},
	} {
		t.Run(tc.name, func(t *testing.T) {
			callbacks = nil
			result := sendStatusRequestWithLocalID(context.Background(), request, client, nil,
				func(*whatsmeow.Client, context.Context, types.JID, *waE2E.Message) (whatsmeow.SendResponse, error) {
					return tc.response, tc.err
				}, tc.localID, onSent)
			if len(callbacks) != 0 {
				t.Fatalf("unconfirmed send invoked callback: %+v", callbacks)
			}
			if tc.err != nil && result != statusSendResultSendFailed || tc.err == nil && result != statusSendResultSent {
				t.Fatalf("result = %d, error = %v", result, tc.err)
			}
		})
	}
}

func ptr[T any](value T) *T {
	return &value
}

func TestOutboundWirePayloadCarriesTextMentionMetadata(t *testing.T) {
	message := contentToWaE2EMessage(
		MessageTypeText,
		"hello @123",
		[]string{"123@lid"},
		0,
		"",
		nil,
		&waE2E.ContextInfo{},
		nil,
	)
	contextInfo := message.GetExtendedTextMessage().GetContextInfo()
	if got := contextInfo.GetMentionedJID(); len(got) != 1 || got[0] != "123@lid" {
		t.Fatalf("text protobuf MentionedJID = %v, want [123@lid]", got)
	}
}

func TestOutboundWirePayloadCarriesCaptionMentionMetadata(t *testing.T) {
	file, err := os.CreateTemp(t.TempDir(), "mention-image.png")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString("image"); err != nil {
		t.Fatal(err)
	}
	file.Close()

	caption := "hello @123"
	message := contentToWaE2EMessage(
		MessageTypeFile,
		"",
		[]string{"123@s.whatsapp.net"},
		FileTypeImage,
		file.Name(),
		&caption,
		&waE2E.ContextInfo{},
		func(context.Context, []byte, whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
			return whatsmeow.UploadResponse{URL: "https://example.test/image", DirectPath: "/image"}, nil
		},
	)
	contextInfo := message.GetImageMessage().GetContextInfo()
	if got := contextInfo.GetMentionedJID(); len(got) != 1 || got[0] != "123@s.whatsapp.net" {
		t.Fatalf("caption protobuf MentionedJID = %v, want [123@s.whatsapp.net]", got)
	}
}

func TestSetMentionedJIDsPreservesFileCaptionMetadata(t *testing.T) {
	contextInfo := &waE2E.ContextInfo{}
	setMentionedJIDsFromStrings(contextInfo, []string{"111@s.whatsapp.net"})

	if got := contextInfo.GetMentionedJID(); len(got) != 1 || got[0] != "111@s.whatsapp.net" {
		t.Fatalf("MentionedJID = %v, want [111@s.whatsapp.net]", got)
	}
}

func TestSetMentionedJIDsAcceptsNullContext(t *testing.T) {
	setMentionedJIDsFromStrings(nil, []string{"111@s.whatsapp.net"})
}

func TestQuotedMessageFromContentPreservesTextAndFileKindsWithoutUpload(t *testing.T) {
	quotedText := quotedTextMessage("quoted text")
	if quotedText.GetConversation() != "quoted text" {
		t.Fatalf("quoted text = %#v", quotedText)
	}

	quotedImage := quotedFileMessage(FileTypeImage, "caption")
	if quotedImage.GetImageMessage() == nil || quotedImage.GetImageMessage().GetCaption() != "caption" {
		t.Fatalf("quoted image = %#v", quotedImage)
	}
	if quotedFileMessage(99, "") != nil {
		t.Fatal("unknown quoted file kind must be omitted")
	}
}

func TestQuotedMessageBuildersPreserveTextAndFileKinds(t *testing.T) {
	if quotedTextMessage("quoted text").GetConversation() != "quoted text" {
		t.Fatal("quoted text was not preserved")
	}
	if quotedFileMessage(FileTypeImage, "caption").GetImageMessage().GetCaption() != "caption" {
		t.Fatal("image quote caption was not preserved")
	}
	if quotedFileMessage(FileTypeAudio, "ignored").GetAudioMessage() == nil {
		t.Fatal("audio quote was not built")
	}
	if quotedFileMessage(99, "caption") != nil {
		t.Fatal("unknown quoted file kind should be omitted")
	}
}

func TestQuotedContextInfoPreservesOriginalAttribution(t *testing.T) {
	context := quotedContextInfo("message-id", "sender@s.whatsapp.net", "chat@s.whatsapp.net")
	if context.GetStanzaID() != "message-id" || context.GetParticipant() != "sender@s.whatsapp.net" || context.GetRemoteJID() != "chat@s.whatsapp.net" {
		t.Fatalf("quote context = %+v, want original attribution", context)
	}
}

func TestMessageSendSeparatesOutboundResponsibilities(t *testing.T) {
	tests := []struct {
		name      string
		file      string
		fragments []string
	}{
		{
			name: "C ABI decoding and exported bridge stay together",
			file: "message_send.go",
			fragments: []string{
				"//export C_SendMessage",
				"func C_SendMessage(cjid C.JID",
				"//export C_SendTextMessage",
				"func C_SendTextMessage(cjid C.JID",
				"//export C_SendOutboundMessage",
				"func C_SendOutboundMessage(cjid C.JID",
				"C_SendOutboundMessage(cjid, messageType",
				"func textSendRequestFromC(",
			},
		},
		{
			name: "message construction and upload are isolated",
			file: "message_send_build.go",
			fragments: []string{
				"func buildOutboundMessage(",
				"func contentToWaE2EMessage(",
				"buildFileMessage(",
			},
		},
		{
			name: "transport and status mapping are isolated",
			file: "message_send_transport.go",
			fragments: []string{
				"func sendOutboundRequestWithContext(",
				"requestSendMessage(clientSnapshot, sendContext, request.chat, message)",
				"func contextStatus(",
			},
		},
		{
			name: "callbacks are isolated",
			file: "message_send_callback.go",
			fragments: []string{
				"func emitOutboundCallback(",
				"optimisticTextSentCallback(request.localSendID, messageInfo, message)",
				"normalMessageCallback(messageInfo, message, false)",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			source, err := os.ReadFile(tt.file)
			if err != nil {
				t.Fatal(err)
			}
			for _, fragment := range tt.fragments {
				if !strings.Contains(string(source), fragment) {
					t.Fatalf("%s missing %q", tt.file, fragment)
				}
			}
		})
	}
}

func TestOptimisticTextSendUsesRequestScopedCallbackInsteadOfGenericLocalState(t *testing.T) {
	source, err := os.ReadFile("message_send_callback.go")
	if err != nil {
		t.Fatal(err)
	}
	text := string(source)
	for _, fragment := range []string{
		"request.localSendID != 0",
		"optimisticTextSentCallback(request.localSendID, messageInfo, message)",
	} {
		if !strings.Contains(text, fragment) {
			t.Fatalf("optimistic text send missing request-scoped fragment %q", fragment)
		}
	}
	if strings.Contains(text, "activeLocalSendID") {
		t.Fatal("optimistic text send must not use process-global local-send state")
	}
}

func TestOptimisticTextSendContextBoundsBlockingSendWithoutFiveSecondSleep(t *testing.T) {
	ctx, cancel := optimisticTextSendContext(context.Background(), 10*time.Millisecond)
	defer cancel()
	blockingSend := func(ctx context.Context) error {
		<-ctx.Done()
		return ctx.Err()
	}
	if err := blockingSend(ctx); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("blocking send error = %v, want deadline exceeded", err)
	}
	if ctx.Err() == nil {
		t.Fatal("bounded context did not finish")
	}
}

func TestOptimisticTextSendContextHonorsCancellation(t *testing.T) {
	ctx, cancel := optimisticTextSendContext(context.Background(), optimisticTextSendTimeout)
	cancel()
	if err := ctx.Err(); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled context error = %v, want context canceled", err)
	}
}

func TestOutboundSendStatusContractIsStable(t *testing.T) {
	if outboundSendSent != 0 || outboundSendInvalidRequest != 1 || outboundSendClientUnavailable != 2 || outboundSendPreparationFailed != 3 || outboundSendTimedOut != 4 || outboundSendCancelled != 5 || outboundSendTransportFailed != 6 {
		t.Fatalf("unexpected outbound status mapping: %d %d %d %d %d %d %d", outboundSendSent, outboundSendInvalidRequest, outboundSendClientUnavailable, outboundSendPreparationFailed, outboundSendTimedOut, outboundSendCancelled, outboundSendTransportFailed)
	}
}

func TestUnifiedOutboundSendMapsPreparationAndTransportOutcomes(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	previousSend := requestSendMessage
	t.Cleanup(func() {
		lifecycleState.publishClient(previousClient)
		requestSendMessage = previousSend
	})

	self := types.NewJID("self", types.DefaultUserServer)
	client := &whatsmeow.Client{Store: &store.Device{ID: &self}}
	lifecycleState.publishClient(client)
	request := textSendRequest{messageType: MessageTypeText, chat: types.NewJID("chat", types.DefaultUserServer), text: "hello"}

	requestSendMessage = func(_ *whatsmeow.Client, _ context.Context, _ types.JID, _ *waE2E.Message) (whatsmeow.SendResponse, error) {
		return whatsmeow.SendResponse{}, errors.New("transport")
	}
	if got := sendOutboundRequest(request); got != outboundSendTransportFailed {
		t.Fatalf("transport status = %d, want %d", got, outboundSendTransportFailed)
	}

	request.messageType = MessageTypeFile
	request.fileKind = FileTypeImage
	request.filePath = "missing-file"
	if got := sendOutboundRequest(request); got != outboundSendPreparationFailed {
		t.Fatalf("preparation status = %d, want %d", got, outboundSendPreparationFailed)
	}
}

func TestUnifiedOutboundSendBoundsPreparationAndTransport(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	previousSend := requestSendMessage
	previousCallback := normalMessageCallback
	t.Cleanup(func() {
		lifecycleState.publishClient(previousClient)
		requestSendMessage = previousSend
		normalMessageCallback = previousCallback
	})

	file, err := os.CreateTemp(t.TempDir(), "outbound-image.png")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := file.WriteString("image"); err != nil {
		t.Fatal(err)
	}
	if err := file.Close(); err != nil {
		t.Fatal(err)
	}
	preparationContext, cancel := optimisticTextSendContext(context.Background(), optimisticTextSendTimeout)
	defer cancel()
	uploadBounded := false
	fileRequest := textSendRequest{messageType: MessageTypeFile, fileKind: FileTypeImage, filePath: file.Name()}
	if _, err := buildOutboundMessage(preparationContext, fileRequest, &waE2E.ContextInfo{}, func(ctx context.Context, _ []byte, _ whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
		_, uploadBounded = ctx.Deadline()
		return whatsmeow.UploadResponse{URL: "https://example.test/image", DirectPath: "/image"}, nil
	}); err != nil || !uploadBounded {
		t.Fatalf("file preparation error/deadline = %v/%v, want nil/true", err, uploadBounded)
	}

	self := types.NewJID("self", types.DefaultUserServer)
	lifecycleState.publishClient(&whatsmeow.Client{Store: &store.Device{ID: &self}})
	requestSendMessage = func(_ *whatsmeow.Client, ctx context.Context, _ types.JID, _ *waE2E.Message) (whatsmeow.SendResponse, error) {
		if _, ok := ctx.Deadline(); !ok {
			t.Fatal("transport did not receive the outbound deadline")
		}
		return whatsmeow.SendResponse{ID: "sent"}, nil
	}
	normalMessageCallback = func(types.MessageInfo, *waE2E.Message, bool) {}
	request := textSendRequest{messageType: MessageTypeText, chat: types.NewJID("chat", types.DefaultUserServer), text: "hello"}
	if got := sendOutboundRequest(request); got != outboundSendSent {
		t.Fatalf("text send status = %d, want %d", got, outboundSendSent)
	}
}

func TestUnifiedOutboundSendReportsSuccessWithStableStatus(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	previousSend := requestSendMessage
	previousCallback := normalMessageCallback
	t.Cleanup(func() {
		lifecycleState.publishClient(previousClient)
		requestSendMessage = previousSend
		normalMessageCallback = previousCallback
	})

	self := types.NewJID("self", types.DefaultUserServer)
	lifecycleState.publishClient(&whatsmeow.Client{Store: &store.Device{ID: &self}})
	requestSendMessage = func(_ *whatsmeow.Client, _ context.Context, _ types.JID, _ *waE2E.Message) (whatsmeow.SendResponse, error) {
		return whatsmeow.SendResponse{ID: "sent"}, nil
	}
	called := false
	normalMessageCallback = func(types.MessageInfo, *waE2E.Message, bool) { called = true }
	request := textSendRequest{messageType: MessageTypeText, chat: types.NewJID("chat", types.DefaultUserServer), text: "hello"}
	if got := sendOutboundRequest(request); got != outboundSendSent || !called {
		t.Fatalf("success status/callback = %d/%v, want %d/true", got, called, outboundSendSent)
	}
}

func TestUnifiedOutboundSendMapsUnavailableAndContextErrors(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	previousSend := requestSendMessage
	t.Cleanup(func() {
		lifecycleState.publishClient(previousClient)
		requestSendMessage = previousSend
	})

	request := textSendRequest{messageType: MessageTypeText, chat: types.NewJID("chat", types.DefaultUserServer), text: "hello"}
	lifecycleState.publishClient(nil)
	if got := sendOutboundRequest(request); got != outboundSendClientUnavailable {
		t.Fatalf("unavailable status = %d, want %d", got, outboundSendClientUnavailable)
	}

	self := types.NewJID("self", types.DefaultUserServer)
	client := &whatsmeow.Client{Store: &store.Device{ID: &self}}
	lifecycleState.publishClient(client)
	requestSendMessage = func(_ *whatsmeow.Client, ctx context.Context, _ types.JID, _ *waE2E.Message) (whatsmeow.SendResponse, error) {
		return whatsmeow.SendResponse{}, ctx.Err()
	}
	if got := sendOutboundRequestWithContext(contextWithError(context.DeadlineExceeded), request); got != outboundSendTimedOut {
		t.Fatalf("timeout status = %d, want %d", got, outboundSendTimedOut)
	}
	if got := sendOutboundRequestWithContext(contextWithError(context.Canceled), request); got != outboundSendCancelled {
		t.Fatalf("cancelled status = %d, want %d", got, outboundSendCancelled)
	}
}

type errorContext struct {
	context.Context
	err error
}

func (ctx errorContext) Err() error { return ctx.err }

func contextWithError(err error) context.Context {
	return errorContext{Context: context.Background(), err: err}
}

func TestOutboundSendCallersUseLifecycleClientSnapshots(t *testing.T) {
	for _, file := range []string{"message_send.go", "message_actions.go", "forwarding_orchestration.go"} {
		source, err := os.ReadFile(file)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.Contains(string(source), "lifecycleState.clientSnapshot()") {
			t.Fatalf("%s does not capture a lifecycle client snapshot", file)
		}
	}
}
