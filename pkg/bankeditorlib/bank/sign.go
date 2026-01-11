package bank

import (
	"crypto/sha1"
	"encoding/hex"
	"errors"
	"starbank/pkg/bankeditorlib/util"
	"strings"
)

func UpdateSignature(bank *Bank) error {
	if bank == nil {
		return errors.New("Invalid bank")
	}

	var data strings.Builder

	data.WriteString(bank.BankInfo.AuthorID)
	data.WriteString(bank.BankInfo.PlayerID)
	data.WriteString(bank.BankInfo.Name)

	for _, section := range util.OrderedByNameAscii(bank.Sections) {
		data.WriteString(section.Name)

		for _, key := range util.OrderedByNameAscii(section.Keys) {
			data.WriteString(key.Name)
			data.WriteString("Value")
			data.WriteString(key.Type)
			data.WriteString(key.Value)
		}
	}

	bank.Signature = generateChecksum(data.String())

	return nil
}

// Signature format: Capitalized hex value of SHA1 checksum of above input data
func generateChecksum(hashMe string) string {
	hashed := sha1.Sum([]byte(hashMe))
	return strings.ToUpper(hex.EncodeToString(hashed[:]))
}
