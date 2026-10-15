package main

import (
	"context"
	"reflect"
	"strings"
	"testing"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
)

type mentionContactStore struct{}

type mentionDirectContactStore struct {
	mentionContactStore
	direct map[types.JID]types.ContactInfo
}

func (s mentionDirectContactStore) GetContact(_ context.Context, jid types.JID) (types.ContactInfo, error) {
	return s.direct[jid], nil
}

func (s mentionDirectContactStore) GetAllContacts(context.Context) (map[types.JID]types.ContactInfo, error) {
	return map[types.JID]types.ContactInfo{
		{User: "789", Server: types.DefaultUserServer}: {FullName: "Unrelated Generic Contact"},
	}, nil
}

type mentionPrefixedContactStore struct {
	mentionContactStore
}

func (mentionPrefixedContactStore) GetAllContacts(context.Context) (map[types.JID]types.ContactInfo, error) {
	return map[types.JID]types.ContactInfo{
		{User: "123", Server: types.DefaultUserServer}: {PushName: "Profile"},
		{User: "456", Server: types.DefaultUserServer}: {BusinessName: "Business"},
		{User: "789", Server: types.DefaultUserServer}: {FullName: "Wrong Generic Name"},
	}, nil
}

func (mentionContactStore) PutPushName(context.Context, types.JID, string) (bool, string, error) {
	return false, "", nil
}

func (mentionContactStore) PutBusinessName(context.Context, types.JID, string) (bool, string, error) {
	return false, "", nil
}

func (mentionContactStore) PutContactName(context.Context, types.JID, string, string) error {
	return nil
}

func (mentionContactStore) PutAllContactNames(context.Context, []store.ContactEntry) error {
	return nil
}

func (mentionContactStore) PutManyRedactedPhones(context.Context, []store.RedactedPhoneEntry) error {
	return nil
}

func (mentionContactStore) GetContact(context.Context, types.JID) (types.ContactInfo, error) {
	return types.ContactInfo{}, nil
}

func (mentionContactStore) GetAllContacts(context.Context) (map[types.JID]types.ContactInfo, error) {
	return map[types.JID]types.ContactInfo{
		{User: "123", Server: types.DefaultUserServer}: {FullName: "Alice"},
	}, nil
}

func TestTextWithMentionNamesResolvesSynchronizedMessages(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	lifecycleState.publishClient(&whatsmeow.Client{Store: &store.Device{Contacts: mentionContactStore{}}})
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	contextInfo := &waE2E.ContextInfo{MentionedJID: []string{"123@s.whatsapp.net"}}
	if got := textWithMentionNames("hello @123", contextInfo); got != "hello @Alice" {
		t.Fatalf("textWithMentionNames() = %q, want synchronized mention resolved", got)
	}
}

func TestReplaceMentionedNames(t *testing.T) {
	tests := []struct {
		name      string
		text      string
		mentioned []string
		contacts  []contactEntry
		want      string
	}{
		{
			name:     "no context preserves text",
			text:     "hello @123 and @456",
			contacts: []contactEntry{{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "Alice"}},
			want:     "hello @123 and @456",
		},
		{
			name:      "multiple and repeated mentions replace only matching tokens",
			text:      "@123 hi @456 @123 @999 @1234",
			mentioned: []string{"123@s.whatsapp.net", "456@lid"},
			contacts: []contactEntry{
				{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "Alice"},
				{jid: types.JID{User: "456", Server: types.HiddenUserServer}, name: "Bob"},
			},
			want: "@Alice hi @Bob @Alice @999 @1234",
		},
		{
			name:      "PN and LID aliases resolve to the same contact name",
			text:      "Replying to @777",
			mentioned: []string{"777@lid"},
			contacts: []contactEntry{
				{jid: types.JID{User: "777", Server: types.DefaultUserServer}, name: "Carol"},
				{jid: types.JID{User: "777", Server: types.HiddenUserServer}, name: "Carol"},
			},
			want: "Replying to @Carol",
		},
		{
			name:      "malformed metadata and unusable names preserve numeric tokens",
			text:      "@bad @888 @999",
			mentioned: []string{"not-a-jid", "888@s.whatsapp.net", "999@s.whatsapp.net"},
			contacts: []contactEntry{
				{jid: types.JID{User: "888", Server: types.DefaultUserServer}, name: "  "},
			},
			want: "@bad @888 @999",
		},
		{
			name:      "tokens require boundaries and preserve punctuation",
			text:      "abc@123 @123, @123.",
			mentioned: []string{"123@s.whatsapp.net"},
			contacts: []contactEntry{
				{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "Alice"},
			},
			want: "abc@123 @Alice, @Alice.",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := replaceMentionedNames(tt.text, tt.mentioned, tt.contacts); got != tt.want {
				t.Fatalf("replaceMentionedNames() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestGroupMentionMetadataFailureUsesOnlyDirectSavedContactLookup(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	client := whatsmeow.NewClient(&store.Device{Contacts: mentionDirectContactStore{direct: map[types.JID]types.ContactInfo{
		{User: "123", Server: types.DefaultUserServer}: {FullName: "Saved Direct Name"},
	}}}, nil)
	lifecycleState.publishClient(client)
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	group := types.NewJID("12345", types.GroupServer)
	mentioned := []string{
		(types.JID{User: "123", Server: types.DefaultUserServer}).String(),
		(types.JID{User: "789", Server: types.DefaultUserServer}).String(),
	}

	entries := mentionEntriesForGroup(ctx, group, mentioned...)
	got, ranges := replaceMentionedNamesWithRanges("hello @123 @789", mentioned, entries)
	if got != "hello @Saved Direct Name @789" {
		t.Fatalf("group mention rendering after metadata failure = %q, want direct saved name only", got)
	}
	if len(ranges) != 1 || ranges[0].Start != len("hello ") || ranges[0].End != len("hello @Saved Direct Name") {
		t.Fatalf("group mention ranges after metadata failure = %#v, want final UTF-8 range", ranges)
	}
}

func TestGroupMentionMetadataFailurePreservesUnresolvedTokens(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	client := whatsmeow.NewClient(&store.Device{Contacts: mentionPrefixedContactStore{}}, nil)
	lifecycleState.publishClient(client)
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	group := types.NewJID("12345", types.GroupServer)
	mentioned := []string{
		(types.JID{User: "123", Server: types.DefaultUserServer}).String(),
		(types.JID{User: "456", Server: types.DefaultUserServer}).String(),
		(types.JID{User: "789", Server: types.DefaultUserServer}).String(),
	}

	entries := mentionEntriesForGroup(ctx, group)
	got, ranges := replaceMentionedNamesWithRanges("hello @123 @456 @789", mentioned, entries)
	if got != "hello @123 @456 @789" {
		t.Fatalf("group mention rendering after metadata failure = %q, want unresolved tokens", got)
	}
	if len(ranges) != 0 {
		t.Fatalf("group mention ranges after metadata failure = %#v, want none", ranges)
	}
	for _, forbidden := range []string{"@~ Profile", "@+ Business", "@Wrong Generic Name"} {
		if strings.Contains(got, forbidden) {
			t.Fatalf("group mention rendering used forbidden generic label %q: %q", forbidden, got)
		}
	}
}

func TestIncomingMentionRenderingResolvesMappedLIDsSafely(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	lid := types.JID{User: "269595130773675", Server: types.HiddenUserServer}
	pn := types.JID{User: "141270097854639", Server: types.DefaultUserServer}
	tests := []struct {
		name        string
		mappedPN    types.JID
		participant types.GroupParticipant
		contacts    groupParticipantContacts
		want        string
	}{
		{
			name:     "saved local name wins for a mapped LID",
			mappedPN: pn,
			participant: types.GroupParticipant{
				JID:         lid,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				pn: {FullName: "Saved Full Name"},
			}},
			want: "hello @Saved Full Name",
		},
		{
			name: "unresolved LID remains numeric",
			participant: types.GroupParticipant{
				JID:         lid,
				DisplayName: "WhatsApp Profile",
			},
			contacts: groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
				pn: {FullName: "Unrelated Contact"},
			}},
			want: "hello @269595130773675",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			lifecycleState.publishClient(&whatsmeow.Client{Store: &store.Device{LIDs: mentionLIDStore{pn: tt.mappedPN}}})
			name := groupParticipantName(context.Background(), tt.participant, tt.contacts)
			got := replaceMentionedNames(
				"hello @"+lid.User,
				[]string{lid.String()},
				[]contactEntry{{jid: pn, name: name}},
			)
			if got != tt.want {
				t.Fatalf("incoming mention rendering = %q, want %q (participant name %q)", got, tt.want, name)
			}
		})
	}
}

type mentionLIDStore struct {
	pn  types.JID
	lid types.JID
}

func (mentionLIDStore) PutManyLIDMappings(context.Context, []store.LIDMapping) error {
	return nil
}

func (mentionLIDStore) PutLIDMapping(context.Context, types.JID, types.JID) error {
	return nil
}

func (s mentionLIDStore) GetPNForLID(context.Context, types.JID) (types.JID, error) {
	return s.pn, nil
}

func (s mentionLIDStore) GetLIDForPN(context.Context, types.JID) (types.JID, error) {
	return s.lid, nil
}

func (mentionLIDStore) GetManyLIDsForPNs(context.Context, []types.JID) (map[types.JID]types.JID, error) {
	return nil, nil
}

func TestOutgoingMentionEchoMapsDifferentlyNumberedPNBodyToLIDMetadata(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	lifecycleState.publishClient(&whatsmeow.Client{Store: &store.Device{LIDs: mentionLIDStore{
		pn: types.JID{User: "141270097854639", Server: types.DefaultUserServer},
	}}})
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	lid := types.JID{User: "269595130773675", Server: types.HiddenUserServer}
	pn := types.JID{User: "141270097854639", Server: types.DefaultUserServer}
	gotText, gotRanges, _ := replaceMentionedNamesWithContextRanges(
		context.Background(), "Hi @141270097854639", []string{lid.String()},
		[]contactEntry{{jid: pn, name: "阿丽"}},
	)
	if gotText != "Hi @阿丽" {
		t.Fatalf("outgoing mention echo = %q, want PN body token replaced", gotText)
	}
	want := []mentionRange{{Start: 3, End: 10}}
	if !reflect.DeepEqual(gotRanges, want) {
		t.Fatalf("outgoing mention ranges = %#v, want final UTF-8 range %#v", gotRanges, want)
	}
}

func TestResolvedMentionRangesUseFinalUTF8OffsetsAndSkipUnresolvedTokens(t *testing.T) {
	gotText, gotRanges := replaceMentionedNamesWithRanges(
		"Hi @123, café @123 and @999!",
		[]string{"123@s.whatsapp.net", "999@s.whatsapp.net"},
		[]contactEntry{{jid: types.JID{User: "123", Server: types.DefaultUserServer}, name: "阿丽"}},
	)
	if gotText != "Hi @阿丽, café @阿丽 and @999!" {
		t.Fatalf("resolved text = %q", gotText)
	}
	want := []mentionRange{{Start: 3, End: 10}, {Start: 18, End: 25}}
	if !reflect.DeepEqual(gotRanges, want) {
		t.Fatalf("ranges = %#v, want %#v", gotRanges, want)
	}
}

func TestMentionJIDMatchesNormalizesADAndPNLIDAliases(t *testing.T) {
	tests := []struct {
		name        string
		left, right types.JID
		want        bool
	}{
		{name: "unmapped phone and lid user aliases remain unresolved", left: types.JID{User: "141270097854639", Server: types.HiddenUserServer}, right: types.JID{User: "141270097854639", Server: types.DefaultUserServer}, want: false},
		{name: "AD suffix does not change identity", left: types.JID{User: "123", Server: types.DefaultUserServer, RawAgent: 1, Device: 2}, right: types.JID{User: "123", Server: types.DefaultUserServer}, want: true},
		{name: "different user remains unresolved", left: types.JID{User: "123", Server: types.DefaultUserServer}, right: types.JID{User: "456", Server: types.DefaultUserServer}, want: false},
		{name: "group IDs never match user aliases", left: types.JID{User: "123", Server: types.GroupServer}, right: types.JID{User: "123", Server: types.DefaultUserServer}, want: false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := mentionJIDMatches(tt.left, tt.right); got != tt.want {
				t.Fatalf("mentionJIDMatches(%v, %v) = %v, want %v", tt.left, tt.right, got, tt.want)
			}
		})
	}
}

func TestCanonicalMentionNamesPreferGroupLabelsOverGenericHelperPrefixes(t *testing.T) {
	jid := types.JID{User: "123", Server: types.DefaultUserServer}
	got := replaceMentionedNames(
		"hello @123",
		[]string{jid.String()},
		[]contactEntry{
			{jid: jid, name: "~ Leo"},
			{jid: jid, name: "Leo"},
		},
	)
	if got != "hello @Leo" {
		t.Fatalf("canonical mention rendering = %q, want helper prefix removed", got)
	}
}

func TestCanonicalMentionNamesKeepUnsavedProfileName(t *testing.T) {
	participant := types.GroupParticipant{
		JID:         types.JID{User: "123", Server: types.DefaultUserServer},
		DisplayName: "WhatsApp Profile",
	}
	name := groupParticipantName(context.Background(), participant, groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
		participant.JID: {PushName: "Push Name"},
	}})
	if name != "WhatsApp Profile" {
		t.Fatalf("groupParticipantName() = %q, want participant profile", name)
	}
}

func TestMentionReplacementNeverRendersLegacyNamePrefixes(t *testing.T) {
	jid := types.JID{User: "123", Server: types.DefaultUserServer}
	got := replaceMentionedNames(
		"hello @123",
		[]string{jid.String()},
		[]contactEntry{{jid: jid, name: "+ Profile"}},
	)
	if got != "hello @Profile" {
		t.Fatalf("mention rendering = %q, want plain profile name", got)
	}
}

func TestGroupMentionMetadataFailureResolvesSelfWithPushNameAndUTF8Range(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	pn := types.NewJID("123", types.DefaultUserServer)
	client := whatsmeow.NewClient(&store.Device{
		ID:       &pn,
		PushName: "阿丽",
		Contacts: mentionDirectContactStore{direct: map[types.JID]types.ContactInfo{}},
	}, nil)
	lifecycleState.publishClient(client)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	mentioned := []string{pn.String()}
	entries := mentionEntriesForGroup(ctx, types.NewJID("12345", types.GroupServer), mentioned...)
	got, ranges := replaceMentionedNamesWithRanges("hello @123", mentioned, entries)
	if got != "hello @阿丽" {
		t.Fatalf("self mention rendering = %q, want Store.PushName", got)
	}
	want := []mentionRange{{Start: len("hello "), End: len("hello @阿丽")}}
	if !reflect.DeepEqual(ranges, want) {
		t.Fatalf("self mention ranges = %#v, want final UTF-8 range %#v", ranges, want)
	}
}

func TestGroupMentionMetadataFailureResolvesSelfLIDAndLocalOverride(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	client := whatsmeow.NewClient(&store.Device{
		ID:       &pn,
		LID:      lid,
		PushName: "Connected Profile",
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
		Contacts: mentionDirectContactStore{direct: map[types.JID]types.ContactInfo{
			pn: {FirstName: "Saved Local Name"},
		}},
	}, nil)
	lifecycleState.publishClient(client)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	mentioned := []string{lid.String()}
	entries := mentionEntriesForGroup(ctx, types.NewJID("12345", types.GroupServer), mentioned...)
	got := replaceMentionedNames("hello @456", mentioned, entries)
	if got != "hello @Saved Local Name" {
		t.Fatalf("self LID mention rendering = %q, want local override over Store.PushName", got)
	}
}

func TestGroupMentionMetadataFailureKeepsMissingSelfNameNumeric(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	pn := types.NewJID("123", types.DefaultUserServer)
	client := whatsmeow.NewClient(&store.Device{
		ID:       &pn,
		Contacts: mentionDirectContactStore{direct: map[types.JID]types.ContactInfo{}},
	}, nil)
	lifecycleState.publishClient(client)

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	mentioned := []string{pn.String()}
	entries := mentionEntriesForGroup(ctx, types.NewJID("12345", types.GroupServer), mentioned...)
	got := replaceMentionedNames("hello @123", mentioned, entries)
	if got != "hello @123" {
		t.Fatalf("missing self name rendering = %q, want numeric token", got)
	}
}

func TestSelfMentionEntriesRemainAvailableWhenGroupParticipantsFilterSelf(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	pn := types.NewJID("123", types.DefaultUserServer)
	lid := types.NewJID("456", types.HiddenUserServer)
	client := whatsmeow.NewClient(&store.Device{
		ID:       &pn,
		LID:      lid,
		PushName: "Connected Profile",
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}, nil)
	lifecycleState.publishClient(client)

	entries := selfMentionEntries(context.Background(), client)
	if len(entries) != 2 {
		t.Fatalf("self mention aliases = %#v, want verified PN and LID", entries)
	}
	for _, entry := range entries {
		if entry.name != "Connected Profile" {
			t.Fatalf("self mention entry name = %q, want Store.PushName", entry.name)
		}
	}
}

func TestSelfMentionAliasesUseOneDisplayNameWithoutAmbiguity(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	pn := types.NewJID("141270097854639", types.DefaultUserServer)
	lid := types.NewJID("269595130773675", types.HiddenUserServer)
	client := whatsmeow.NewClient(&store.Device{
		ID:       &pn,
		LID:      lid,
		PushName: "SAMA3L",
		LIDs: participantIdentityLIDStore{
			pnByLID: map[types.JID]types.JID{lid: pn},
			lidByPN: map[types.JID]types.JID{pn: lid},
		},
	}, nil)
	lifecycleState.publishClient(client)
	contacts := groupParticipantContacts{contacts: map[types.JID]types.ContactInfo{
		pn: {FullName: "+593 99 568 2425"},
	}}
	participant := types.GroupParticipant{JID: lid, PhoneNumber: pn, LID: lid}
	participantName := groupParticipantName(context.Background(), participant, contacts)
	entries := []contactEntry{
		{jid: pn, name: participantName},
		{jid: lid, name: participantName},
	}
	entries = append(entries, selfMentionEntries(context.Background(), client)...)

	got, ranges := replaceMentionedNamesWithRanges(
		"hello @141270097854639",
		[]string{pn.String()},
		entries,
	)
	if got != "hello @SAMA3L" {
		t.Fatalf("self mention rendering = %q, want profile PushName", got)
	}
	wantRange := mentionRange{Start: len("hello "), End: len("hello @SAMA3L")}
	if !reflect.DeepEqual(ranges, []mentionRange{wantRange}) {
		t.Fatalf("self mention ranges = %#v, want %#v", ranges, []mentionRange{wantRange})
	}
}

func TestAssignMentionNameKeepsRepeatedSelfAliasEntriesUnambiguous(t *testing.T) {
	names := map[string]string{}
	ambiguous := map[string]bool{}
	assignMentionName(names, ambiguous, "141270097854639", "SAMA3L")
	assignMentionName(names, ambiguous, "141270097854639", "SAMA3L")

	if ambiguous["141270097854639"] {
		t.Fatal("repeated self alias entries must not be marked ambiguous")
	}
	if got := names["141270097854639"]; got != "SAMA3L" {
		t.Fatalf("self alias name = %q, want PushName", got)
	}
}

func TestAssignMentionNameFailsClosedForRealAmbiguity(t *testing.T) {
	names := map[string]string{}
	ambiguous := map[string]bool{}
	assignMentionName(names, ambiguous, "123", "Alice")
	assignMentionName(names, ambiguous, "123", "Bob")

	if !ambiguous["123"] {
		t.Fatal("different names for one unresolved alias must remain ambiguous")
	}
	if _, ok := names["123"]; ok {
		t.Fatal("ambiguous alias must not retain a replacement")
	}
}

func TestSelfDisplayNamePrefersSavedNameThenPushName(t *testing.T) {
	previousClient := lifecycleState.clientSnapshot()
	t.Cleanup(func() { lifecycleState.publishClient(previousClient) })

	pn := types.NewJID("123", types.DefaultUserServer)
	client := whatsmeow.NewClient(&store.Device{
		ID:       &pn,
		PushName: "Connected Profile",
		Contacts: mentionDirectContactStore{direct: map[types.JID]types.ContactInfo{
			pn: {FirstName: "Saved Local Name"},
		}},
	}, nil)
	lifecycleState.publishClient(client)

	if got := selfDisplayName(context.Background(), client); got != "Saved Local Name" {
		t.Fatalf("selfDisplayName() = %q, want saved local name", got)
	}
	client.Store.Contacts = mentionContactStore{}
	if got := selfDisplayName(context.Background(), client); got != "Connected Profile" {
		t.Fatalf("selfDisplayName() = %q, want Store.PushName", got)
	}
}

func TestPendingMentionRangesRetainRepeatedIdenticalMessages(t *testing.T) {
	pendingMentionRangesStore.Lock()
	pendingMentionRangesStore.values = make(map[string][]pendingMentionRanges)
	pendingMentionRangesStore.Unlock()
	t.Cleanup(func() {
		pendingMentionRangesStore.Lock()
		pendingMentionRangesStore.values = make(map[string][]pendingMentionRanges)
		pendingMentionRangesStore.Unlock()
	})

	rememberPendingMentionRanges("same", "same", []mentionRange{{Start: 0, End: 5}})
	rememberPendingMentionRanges("same", "same", []mentionRange{{Start: 6, End: 11}})

	if got := takePendingMentionRanges("same"); !reflect.DeepEqual(got, []mentionRange{{Start: 0, End: 5}}) {
		t.Fatalf("first repeated pending ranges = %#v", got)
	}
	if got := takePendingMentionRanges("same"); !reflect.DeepEqual(got, []mentionRange{{Start: 6, End: 11}}) {
		t.Fatalf("second repeated pending ranges = %#v", got)
	}
}
