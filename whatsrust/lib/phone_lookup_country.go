package main

import "go.mau.fi/whatsmeow/types"

// countryEquivalentPhoneIdentity accepts only a server PN with the same identity
// metadata as the request and one of the two narrow country-number differences.
// Exact matches are handled first by lookupPhone, across both response fields.
func countryEquivalentPhoneIdentity(identity, requested types.JID) (types.JID, bool) {
	identity = identity.ToNonAD()
	metadata := identity
	metadata.User = requested.User
	if !matchesRequestedPhoneIdentity(metadata, requested) || !equivalentCountryPhoneDigits(identity.User, requested.User) {
		return types.JID{}, false
	}
	if identity.Server == types.LegacyUserServer {
		identity.Server = types.DefaultUserServer
	}
	return identity, true
}

// equivalentCountryPhoneDigits is symmetric: the shorter spelling must be a
// country code plus exactly ten national digits, and the longer spelling must
// insert only that country's mobile marker before the identical national digits.
// This does not interpret domestic prefixes, remove 15/zero, or guess area codes.
func equivalentCountryPhoneDigits(first, second string) bool {
	if len(first) > len(second) {
		first, second = second, first
	}
	if len(first) != 12 || len(second) != 13 {
		return false
	}
	for _, digits := range []string{first, second} {
		for _, digit := range digits {
			if digit < '0' || digit > '9' {
				return false
			}
		}
	}
	if first[:2] != second[:2] || first[2:] != second[3:] {
		return false
	}
	return (first[:2] == "52" && second[2] == '1') ||
		(first[:2] == "54" && second[2] == '9')
}
