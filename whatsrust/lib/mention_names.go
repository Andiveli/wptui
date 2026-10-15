package main

import (
	"context"
	"sort"
	"strings"
	"sync"
	"unicode"
	"unicode/utf8"

	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
)

type mentionRange struct {
	Start int
	End   int
}

type mentionEdit struct {
	Start       int
	End         int
	replacement string
}

type pendingMentionRanges struct {
	ranges []mentionRange
}

var pendingMentionRangesStore = struct {
	sync.Mutex
	values map[string][]pendingMentionRanges
}{values: make(map[string][]pendingMentionRanges)}

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
	resolved, ranges, _ := replaceMentionedNamesWithContextRanges(
		context.Background(), text, mentionedJIDs, contacts,
	)
	rememberPendingMentionRanges(text, resolved, ranges)
	return resolved
}

func replaceMentionedNames(text string, mentionedJIDs []string, contacts []contactEntry) string {
	resolved, ranges, edits := replaceMentionedNamesWithContextRanges(
		context.Background(), text, mentionedJIDs, contacts,
	)
	if resolved == text {
		return resolved
	}

	previous := takePendingMentionRanges(text)
	transformed := transformMentionRanges(previous, edits)
	transformed = append(transformed, ranges...)
	if len(transformed) > 0 {
		sort.Slice(transformed, func(i, j int) bool {
			return transformed[i].Start < transformed[j].Start
		})
		rememberPendingMentionRanges(resolved, resolved, transformed)
	}
	return resolved
}

func replaceMentionedNamesWithRanges(
	text string, mentionedJIDs []string, contacts []contactEntry,
) (string, []mentionRange) {
	resolved, ranges, _ := replaceMentionedNamesWithContextRanges(
		context.Background(), text, mentionedJIDs, contacts,
	)
	return resolved, ranges
}

func replaceMentionedNamesWithContext(ctx context.Context, text string, mentionedJIDs []string, contacts []contactEntry) string {
	resolved, _, _ := replaceMentionedNamesWithContextRanges(ctx, text, mentionedJIDs, contacts)
	return resolved
}

func replaceMentionedNamesWithContextRanges(
	ctx context.Context, text string, mentionedJIDs []string, contacts []contactEntry,
) (string, []mentionRange, []mentionEdit) {
	if text == "" || len(mentionedJIDs) == 0 || len(contacts) == 0 {
		return text, nil, nil
	}

	names := make(map[string]string, len(mentionedJIDs))
	ambiguous := make(map[string]bool)
	for _, mentionedJID := range mentionedJIDs {
		jid, err := types.ParseJID(mentionedJID)
		if err != nil || jid.IsEmpty() || jid.User == "" {
			continue
		}
		for _, contact := range contacts {
			if !mentionJIDMatchesWithContext(ctx, contact.jid, jid) || strings.TrimSpace(contact.name) == "" {
				continue
			}
			for _, alias := range appendMentionAliases(ctx, jid, contact.jid) {
				assignMentionName(names, ambiguous, alias.User, contact.name)
			}
		}
	}
	if len(names) == 0 {
		return text, nil, nil
	}

	return replaceMentionTokensWithRanges(text, names)
}

// mentionJIDMatches accepts WhatsApp's phone-number, LID and AD variants
// without guessing across unrelated servers.
func mentionJIDMatches(left, right types.JID) bool {
	return mentionJIDMatchesWithContext(context.Background(), left, right)
}

func mentionJIDMatchesWithContext(ctx context.Context, left, right types.JID) bool {
	if left == right || left.ToNonAD() == right.ToNonAD() {
		return true
	}
	clientSnapshot := lifecycleState.clientSnapshot()
	if !isUserJID(left) || !isUserJID(right) || clientSnapshot == nil || clientSnapshot.Store == nil || clientSnapshot.Store.LIDs == nil {
		return false
	}

	lids := clientSnapshot.Store.LIDs
	leftCanonical := left.ToNonAD()
	rightCanonical := right.ToNonAD()
	if leftCanonical.Server == types.HiddenUserServer {
		if pn, err := lids.GetPNForLID(ctx, leftCanonical); err == nil && !pn.IsEmpty() && pn.ToNonAD() == rightCanonical {
			return true
		}
	}
	if leftCanonical.Server == types.DefaultUserServer {
		if lid, err := lids.GetLIDForPN(ctx, leftCanonical); err == nil && !lid.IsEmpty() && lid.ToNonAD() == rightCanonical {
			return true
		}
	}
	if rightCanonical.Server == types.HiddenUserServer {
		if pn, err := lids.GetPNForLID(ctx, rightCanonical); err == nil && !pn.IsEmpty() && pn.ToNonAD() == leftCanonical {
			return true
		}
	}
	if rightCanonical.Server == types.DefaultUserServer {
		if lid, err := lids.GetLIDForPN(ctx, rightCanonical); err == nil && !lid.IsEmpty() && lid.ToNonAD() == leftCanonical {
			return true
		}
	}
	return false
}

func isUserJID(jid types.JID) bool {
	return jid.Server == types.DefaultUserServer || jid.Server == types.HiddenUserServer
}

func mentionEntriesForGroup(ctx context.Context, group types.JID, mentionedJIDs ...string) []contactEntry {
	clientSnapshot := lifecycleState.clientSnapshot()
	if group.Server != types.GroupServer {
		return lookupMentionContactEntries()
	}
	if clientSnapshot == nil || clientSnapshot.Store == nil {
		return nil
	}
	info, err := clientSnapshot.GetGroupInfo(ctx, group.ToNonAD())
	if err != nil || info == nil {
		return directMentionEntries(ctx, mentionedJIDs)
	}
	entries := make([]contactEntry, 0, len(info.Participants))
	participants := deduplicateGroupParticipants(ctx, info.Participants, clientSnapshot.Store.LIDs)
	for _, participant := range participants {
		name := groupParticipantName(ctx, participant, clientSnapshot.Store.Contacts)
		for _, jid := range participantJIDs(ctx, participant, clientSnapshot.Store.LIDs) {
			entries = append(entries, contactEntry{jid: jid, name: name})
		}
	}
	// The picker intentionally filters self, so add the authenticated identity
	// explicitly for incoming mentions. The helper only returns verified aliases.
	entries = append(entries, selfMentionEntries(ctx, clientSnapshot)...)
	return entries
}

func directMentionEntries(ctx context.Context, mentionedJIDs []string) []contactEntry {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil {
		return nil
	}
	entries := make([]contactEntry, 0, len(mentionedJIDs))
	for _, mentionedJID := range mentionedJIDs {
		jid, err := types.ParseJID(mentionedJID)
		if err != nil || jid.IsEmpty() || !isUserJID(jid) {
			continue
		}
		aliases := mentionJIDAliases(ctx, jid)
		name := ""
		if participantMatchesSelf(clientSnapshot, types.GroupParticipant{JID: jid}) {
			name = selfDisplayName(ctx, clientSnapshot)
		}
		if clientSnapshot.Store.Contacts != nil {
			for _, alias := range aliases {
				if name != "" {
					break
				}
				contact, err := clientSnapshot.Store.Contacts.GetContact(ctx, alias)
				if err != nil {
					continue
				}
				if name = locallySavedContactName(contact); name != "" {
					break
				}
			}
		}
		if name == "" {
			continue
		}
		for _, verifiedAlias := range aliases {
			entries = append(entries, contactEntry{jid: verifiedAlias, name: name})
		}
	}
	return entries
}

func mentionJIDAliases(ctx context.Context, jid types.JID) []types.JID {
	clientSnapshot := lifecycleState.clientSnapshot()
	if clientSnapshot == nil || clientSnapshot.Store == nil {
		return []types.JID{jid, jid.ToNonAD()}
	}
	return participantJIDs(ctx, types.GroupParticipant{JID: jid}, clientSnapshot.Store.LIDs)
}

func appendMentionAliases(ctx context.Context, mentioned, contact types.JID) []types.JID {
	aliases := mentionJIDAliases(ctx, mentioned)
	for _, alias := range mentionJIDAliases(ctx, contact) {
		found := false
		for _, existing := range aliases {
			if existing == alias {
				found = true
				break
			}
		}
		if !found {
			aliases = append(aliases, alias)
		}
	}
	return aliases
}

func assignMentionName(names map[string]string, ambiguous map[string]bool, user, name string) {
	name = plainContactName(name)
	if user == "" || name == "" || ambiguous[user] {
		return
	}
	if current, exists := names[user]; !exists {
		names[user] = name
	} else if canonicalMentionLabel(current) == canonicalMentionLabel(name) {
		if mentionNameRank(name) < mentionNameRank(current) {
			names[user] = name
		}
	} else if mentionNameRank(name) < mentionNameRank(current) {
		names[user] = name
	} else if mentionNameRank(name) == mentionNameRank(current) {
		delete(names, user)
		ambiguous[user] = true
	}
}

func canonicalMentionLabel(name string) string {
	return plainContactName(name)
}

func mentionNameRank(name string) int {
	name = strings.TrimSpace(name)
	if strings.HasPrefix(name, "~ ") || strings.HasPrefix(name, "+ ") {
		return 1
	}
	return 0
}

func replaceMentionTokens(text string, names map[string]string) string {
	resolved, _, _ := replaceMentionTokensWithRanges(text, names)
	return resolved
}

func replaceMentionTokensWithRanges(
	text string, names map[string]string,
) (string, []mentionRange, []mentionEdit) {
	var result strings.Builder
	var ranges []mentionRange
	var edits []mentionEdit
	last := 0

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

		if len(edits) == 0 {
			result.Grow(len(text))
		}
		result.WriteString(text[last:at])
		result.WriteByte('@')
		result.WriteString(name)
		ranges = append(ranges, mentionRange{
			Start: result.Len() - len(name) - 1,
			End:   result.Len(),
		})
		edits = append(edits, mentionEdit{
			Start:       at,
			End:         end,
			replacement: "@" + name,
		})
		last = end
		offset = end
	}

	if len(edits) == 0 {
		return text, nil, nil
	}
	result.WriteString(text[last:])
	return result.String(), ranges, edits
}

func transformMentionRanges(ranges []mentionRange, edits []mentionEdit) []mentionRange {
	if len(ranges) == 0 {
		return nil
	}
	result := make([]mentionRange, 0, len(ranges))
	for _, mention := range ranges {
		shift := 0
		for _, edit := range edits {
			if edit.End <= mention.Start {
				shift += len(edit.replacement) - (edit.End - edit.Start)
				continue
			}
			if edit.Start >= mention.End {
				break
			}
			// Existing semantic ranges cannot overlap a still-unresolved token.
			shift = 0
			break
		}
		result = append(result, mentionRange{
			Start: mention.Start + shift,
			End:   mention.End + shift,
		})
	}
	return result
}

func rememberPendingMentionRanges(source, resolved string, ranges []mentionRange) {
	pendingMentionRangesStore.Lock()
	defer pendingMentionRangesStore.Unlock()
	pendingMentionRangesStore.values[resolved] = append(
		pendingMentionRangesStore.values[resolved],
		pendingMentionRanges{ranges: append([]mentionRange(nil), ranges...)},
	)
}

func takePendingMentionRanges(text string) []mentionRange {
	pendingMentionRangesStore.Lock()
	defer pendingMentionRangesStore.Unlock()
	pending := pendingMentionRangesStore.values[text]
	if len(pending) == 0 {
		return nil
	}
	if len(pending) == 1 {
		delete(pendingMentionRangesStore.values, text)
	} else {
		pendingMentionRangesStore.values[text] = pending[1:]
	}
	return append([]mentionRange(nil), pending[0].ranges...)
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
