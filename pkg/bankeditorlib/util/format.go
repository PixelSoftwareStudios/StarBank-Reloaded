package util

import (
	"strings"
)

// NormalizeNumber removes common formatting characters from numeric strings
// Examples:
//
//	"1,000,000" -> "1000000"
//	"1 000 000" -> "1000000"
//	"-123.45" -> "-123.45" (preserves sign and decimal)
//
// This is a simple string manipulation function with no locale awareness
// or float parsing. It only removes commas and spaces.
func NormalizeNumber(input string) string {
	result := strings.ReplaceAll(input, ",", "")
	result = strings.ReplaceAll(result, " ", "")

	return result
}
