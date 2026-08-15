package main

import (
	"os"
	"strings"
	"testing"
)

func TestReactionDispatchOwnsPayloadInCUntilSynchronousCallbackReturns(t *testing.T) {
	source, err := os.ReadFile("event_conversion.go")
	if err != nil {
		t.Fatal(err)
	}

	body, ok := extractFunctionBody(string(source), "func dispatchReactionEvent(reaction reactionEvent)")
	if !ok {
		t.Fatal("dispatchReactionEvent function body not found in event_conversion.go")
	}

	for _, fragment := range []string{
		"func dispatchReactionEvent(reaction reactionEvent)",
		"if eventHandler.callback == nil",
		"(*C.ReactionEvent)(C.malloc(C.sizeof_ReactionEvent))",
		"C.callEventConversionCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeReaction), data: unsafe.Pointer(payload)})",
		"C.free(unsafe.Pointer(payload))",
	} {
		if !strings.Contains(body, fragment) {
			t.Fatalf("reaction dispatch must contain %q", fragment)
		}
	}

	callback := strings.Index(body, "C.callEventConversionCallback(eventHandler, &C.Event{kind: C.uint8_t(EventTypeReaction), data: unsafe.Pointer(payload)})")
	freePayload := strings.Index(body, "C.free(unsafe.Pointer(payload))")
	if callback < 0 || freePayload < 0 || freePayload < callback {
		t.Fatal("reaction payload must remain C-owned until the callback returns")
	}
}

// extractFunctionBody returns the substring of code that spans the function
// whose signature starts with signaturePrefix, from the signature through the
// brace that closes the function body. Braces inside comments and literals do
// not count, so unrelated statements cannot leak into the returned body.
func extractFunctionBody(code, signaturePrefix string) (string, bool) {
	sig := strings.Index(code, signaturePrefix)
	if sig < 0 {
		return "", false
	}
	open := strings.IndexByte(code[sig:], '{')
	if open < 0 {
		return "", false
	}
	depth := 1
	i := sig + open + 1
	for i < len(code) {
		switch code[i] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return code[sig : i+1], true
			}
		case '/':
			if i+1 < len(code) {
				switch code[i+1] {
				case '/':
					nl := strings.IndexByte(code[i:], '\n')
					if nl < 0 {
						return "", false
					}
					i += nl + 1
					continue
				case '*':
					end := strings.Index(code[i+2:], "*/")
					if end < 0 {
						return "", false
					}
					i += end + 4
					continue
				}
			}
		case '"':
			i++
			for i < len(code) && code[i] != '"' {
				if code[i] == '\\' {
					i++
				}
				i++
			}
		case '\'':
			i++
			for i < len(code) && code[i] != '\'' {
				if code[i] == '\\' {
					i++
				}
				i++
			}
		case '`':
			i++
			for i < len(code) && code[i] != '`' {
				i++
			}
		}
		i++
	}
	return "", false
}
