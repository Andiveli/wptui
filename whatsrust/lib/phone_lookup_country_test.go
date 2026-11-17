package main

import (
	"testing"

	"go.mau.fi/whatsmeow/types"
)

// Equivalence is symmetric, not a rewrite of the user's input: only the
// server-confirmed PN is returned. The two spellings must share all ten national
// digits. In particular, Argentina's domestic 15 is not parsed or removed here.
func TestLookupPhoneCountryEquivalence(t *testing.T) {
	lid := types.NewJID("99887766", types.HiddenUserServer)
	for _, country := range []struct {
		name  string
		plain string
		with  string
	}{
		{"Mexico", "522345678901", "5212345678901"},
		{"Argentina", "542345678901", "5492345678901"},
	} {
		t.Run(country.name, func(t *testing.T) {
			for _, direction := range []struct {
				name      string
				requested string
				returned  string
			}{
				{"plain to mobile", country.plain, country.with},
				{"mobile to plain", country.with, country.plain},
			} {
				t.Run(direction.name, func(t *testing.T) {
					phone := "+" + direction.requested
					pn := types.NewJID(direction.returned, types.DefaultUserServer)
					requested := types.NewJID(direction.requested, types.DefaultUserServer)
					tests := []phoneLookupTestCase{
						{
							name:  "exact JID keeps original success ahead of equivalent associated PN",
							phone: phone, query: true,
							responses: []types.IsOnWhatsAppResponse{{Query: phone, IsIn: true, PhoneNumber: pn, JID: requested}},
							want:      phoneLookupResult{status: phoneLookupFound, jid: requested.String()},
						},
						{
							name:  "exact associated PN keeps original success ahead of equivalent JID",
							phone: phone, query: true,
							responses: []types.IsOnWhatsAppResponse{{Query: phone, IsIn: true, PhoneNumber: requested, JID: pn}},
							want:      phoneLookupResult{status: phoneLookupFound, jid: requested.String()},
						},
					}
					for _, slot := range []string{"associated PN", "JID"} {
						for _, namespace := range []string{types.DefaultUserServer, types.LegacyUserServer} {
							for _, device := range []uint16{0, 2} {
								identity := types.JID{User: pn.User, Server: namespace, Device: device}
								response := types.IsOnWhatsAppResponse{Query: phone, IsIn: true, JID: lid}
								if slot == "associated PN" {
									response.PhoneNumber = identity
								} else {
									response.JID = identity
								}
								deviceName := "non-device"
								if device != 0 {
									deviceName = "device"
								}
								tests = append(tests, phoneLookupTestCase{
									name:  slot + "/" + namespace + "/" + deviceName,
									phone: phone, responses: []types.IsOnWhatsAppResponse{response}, query: true,
									want: phoneLookupResult{status: phoneLookupFound, jid: pn.String()},
								})
							}
						}
					}
					// Query matching retains the existing optional leading plus only;
					// country equivalence never relaxes the query correlation guard.
					tests = append(tests, phoneLookupTestCase{
						name: "query without plus", phone: phone, query: true,
						responses: []types.IsOnWhatsAppResponse{{Query: direction.requested, IsIn: true, JID: pn}},
						want:      phoneLookupResult{status: phoneLookupFound, jid: pn.String()},
					})
					runPhoneLookupTests(t, tests)
				})
			}
		})
	}
}

func TestLookupPhoneCountryEquivalencePreservesExactNumbers(t *testing.T) {
	var tests []phoneLookupTestCase
	for _, digits := range []string{
		"522345678901", "5212345678901", "542345678901", "5492345678901",
		"593987654321", // Ecuador: no country-specific transformation.
	} {
		pn := types.NewJID(digits, types.DefaultUserServer)
		tests = append(tests, phoneLookupTestCase{
			name: digits, phone: "+" + digits, query: true,
			responses: []types.IsOnWhatsAppResponse{{Query: "+" + digits, IsIn: true, JID: pn}},
			want:      phoneLookupResult{status: phoneLookupFound, jid: pn.String()},
		})
	}
	runPhoneLookupTests(t, tests)
}

func TestLookupPhoneCountryEquivalenceRejectsOtherDigits(t *testing.T) {
	lid := types.NewJID("99887766", types.HiddenUserServer)
	for _, pair := range []struct {
		name      string
		requested string
		returned  string
	}{
		{"Mexico wrong suffix", "522345678901", "5212345678902"},
		{"Mexico short national number", "52234567890", "521234567890"},
		{"Mexico long national number", "5223456789012", "52123456789012"},
		{"Mexico wrong inserted digit", "522345678901", "5292345678901"},
		{"Mexico missing national digit", "522345678901", "521234567890"},
		{"Argentina wrong suffix", "542345678901", "5492345678902"},
		{"Argentina short national number", "54234567890", "549234567890"},
		{"Argentina long national number", "5423456789012", "54923456789012"},
		{"Argentina wrong inserted digit", "542345678901", "5412345678901"},
		{"Argentina missing national digit", "542345678901", "549234567890"},
		{"Argentina domestic 15 is not stripped", "54112315456789", "5491123456789"},
		{"Argentina domestic zero is not stripped", "5401123456789", "5491123456789"},
		{"Ecuador extra 1", "593987654321", "5931987654321"},
		{"unsupported country extra 1", "552345678901", "5512345678901"},
		{"unsupported country extra 9", "562345678901", "5692345678901"},
		{"cross country", "522345678901", "5492345678901"},
	} {
		t.Run(pair.name, func(t *testing.T) {
			// Rejection must be symmetric too; no suffix/length heuristic may
			// accept one orientation while rejecting the other.
			var tests []phoneLookupTestCase
			for _, reverse := range []bool{false, true} {
				requested, returned := pair.requested, pair.returned
				direction := "forward"
				if reverse {
					requested, returned = returned, requested
					direction = "reverse"
				}
				pn := types.NewJID(returned, types.DefaultUserServer)
				for _, slot := range []string{"associated PN", "JID"} {
					response := types.IsOnWhatsAppResponse{Query: "+" + requested, IsIn: true, JID: lid}
					status := uint8(166) // Existing comparison bits, no accepted recipient.
					if slot == "associated PN" {
						response.PhoneNumber = pn
					} else {
						response.JID = pn
						status = 180
					}
					tests = append(tests, phoneLookupTestCase{
						name: direction + "/" + slot, phone: "+" + requested, query: true,
						responses: []types.IsOnWhatsAppResponse{response}, want: phoneLookupResult{status: status},
					})
				}
			}
			runPhoneLookupTests(t, tests)
		})
	}
}

func TestLookupPhoneCountryEquivalenceRequiresServerGuards(t *testing.T) {
	lid := types.NewJID("99887766", types.HiddenUserServer)
	for _, pair := range []struct {
		name      string
		requested string
		returned  string
	}{
		{"Mexico", "522345678901", "5212345678901"},
		{"Mexico reverse", "5212345678901", "522345678901"},
		{"Argentina", "542345678901", "5492345678901"},
		{"Argentina reverse", "5492345678901", "542345678901"},
	} {
		t.Run(pair.name, func(t *testing.T) {
			phone := "+" + pair.requested
			var tests []phoneLookupTestCase
			for _, slot := range []string{"associated PN", "JID"} {
				for _, guard := range []struct {
					name       string
					server     string
					integrator uint16
					query      string
					registered bool
					status     uint8
				}{
					{"group namespace", types.GroupServer, 0, phone, true, phoneLookupResponseMismatch},
					{"LID namespace", types.HiddenUserServer, 0, phone, true, phoneLookupResponseMismatch},
					{"integrator", types.DefaultUserServer, 1, phone, true, 162},
					{"legacy integrator", types.LegacyUserServer, 1, phone, true, phoneLookupResponseMismatch},
					{"equivalent query is not exact query", types.DefaultUserServer, 0, "+" + pair.returned, true, phoneLookupQueryMismatch},
					{"unrelated query", types.DefaultUserServer, 0, "+15551234567", true, phoneLookupQueryMismatch},
					{"unregistered", types.DefaultUserServer, 0, phone, false, phoneLookupNotRegistered},
				} {
					identity := types.JID{User: pair.returned, Server: guard.server, Device: 2, Integrator: guard.integrator}
					response := types.IsOnWhatsAppResponse{Query: guard.query, IsIn: guard.registered, JID: lid}
					status := guard.status
					if slot == "associated PN" {
						response.PhoneNumber = identity
					} else {
						response.JID = identity
						if guard.name == "integrator" {
							status = 148
						}
						if guard.name == "LID namespace" {
							status = phoneLookupLidOnly
						}
					}
					tests = append(tests, phoneLookupTestCase{
						name: slot + "/" + guard.name, phone: phone, query: true,
						responses: []types.IsOnWhatsAppResponse{response}, want: phoneLookupResult{status: status},
					})
				}
			}
			runPhoneLookupTests(t, tests)
		})
	}
}
