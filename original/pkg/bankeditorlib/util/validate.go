package util

import "regexp"

var validAccountIDPattern = regexp.MustCompile(`^\d-S2-\d-\d+$`)

// If the account ID matches the format "X-S2-X-XXXXX"
func IsValidAccountID(id string) bool {
	return validAccountIDPattern.MatchString(id)
}
