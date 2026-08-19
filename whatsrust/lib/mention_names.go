package main

import (
	"strings"
	"unicode"
	"unicode/utf8"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

// textWithMentionNames resolves mentions for messages with usable metadata and contacts.
func textWithMentionNames(text string, contextInfo *waE2E.ContextInfo) string {
	if contextInfo == nil || text == "" {
		return text
	}
	mentionedJIDs := contextInfo.GetMentionedJID()
	if len(mentionedJIDs) == 0 {
		return text
	}
	contacts := lookupMentionContactEntries()
	return replaceMentionedNames(text, mentionedJIDs, contacts)
}

func replaceMentionedNames(text string, mentionedJIDs []string, contacts []contactEntry) string {
	if text == "" || len(mentionedJIDs) == 0 || len(contacts) == 0 {
		return text
	}

	names := make(map[string]string, len(mentionedJIDs))
	for _, mentionedJID := range mentionedJIDs {
		jid, err := types.ParseJID(mentionedJID)
		if err != nil || jid.IsEmpty() || jid.User == "" {
			continue
		}
		for _, contact := range contacts {
			if contact.jid == jid && strings.TrimSpace(contact.name) != "" {
				if _, exists := names[jid.User]; !exists {
					names[jid.User] = contact.name
				}
				break
			}
		}
	}
	if len(names) == 0 {
		return text
	}

	return replaceMentionTokens(text, names)
}

func replaceMentionTokens(text string, names map[string]string) string {
	var result strings.Builder
	last := 0
	changed := false

	for offset := 0; offset < len(text); {
		relativeAt := strings.IndexByte(text[offset:], '@')
		if relativeAt < 0 {
			break
		}
		at := offset + relativeAt
		if !mentionBoundaryBefore(text, at) {
			offset = at + 1
			continue
		}

		end := at + 1
		for end < len(text) {
			r, size := utf8.DecodeRuneInString(text[end:])
			if !mentionTokenRune(r) {
				break
			}
			end += size
		}
		user := text[at+1 : end]
		name, ok := names[user]
		if !ok {
			offset = end
			continue
		}

		if !changed {
			result.Grow(len(text))
		}
		result.WriteString(text[last:at])
		result.WriteByte('@')
		result.WriteString(name)
		last = end
		changed = true
		offset = end
	}

	if !changed {
		return text
	}
	result.WriteString(text[last:])
	return result.String()
}

func mentionBoundaryBefore(text string, at int) bool {
	if at == 0 {
		return true
	}
	r, _ := utf8.DecodeLastRuneInString(text[:at])
	return !mentionTokenRune(r)
}

func mentionTokenRune(r rune) bool {
	return r == '_' || unicode.IsLetter(r) || unicode.IsDigit(r)
}
