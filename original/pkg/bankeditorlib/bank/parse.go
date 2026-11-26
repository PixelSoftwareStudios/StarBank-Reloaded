package bank

import (
	"encoding/xml"
	"fmt"
	"os"
)

// ParseBankFile reads a .SC2Bank file from disk and parses it into a Bank model.
func ParseBankFile(path string) (*Bank, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading bank file: %w", err)
	}
	return ParseBank(data)
}

// ParseBank parses raw SC2Bank XML bytes into a Bank model.
// Section and key ordering from the source XML is preserved.
func ParseBank(data []byte) (*Bank, error) {
	var xmlData xmlBank
	if err := xml.Unmarshal(data, &xmlData); err != nil {
		return nil, fmt.Errorf("parsing bank XML: %w", err)
	}

	b := NewBank()

	if xmlData.Version != "" {
		b.Version = xmlData.Version
	}
	if xmlData.Signature != nil {
		b.Signature = xmlData.Signature.Value
	}

	for _, xmlSec := range xmlData.Sections {
		section := b.AddSection(xmlSec.Name)
		for _, xmlK := range xmlSec.Keys {
			section.AddKey(xmlK.Name, xmlK.Value.Type, xmlK.Value.Value)
		}
	}

	return b, nil
}
