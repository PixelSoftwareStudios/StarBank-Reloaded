package bank

import (
	"encoding/xml"
	"fmt"
	"strings"
)

// WriteBank writes a Bank model to XML format
// Preserves section and key order from the model
func WriteBank(bank *Bank) ([]byte, error) {
	if bank == nil {
		return nil, fmt.Errorf("bank cannot be nil")
	}

	// Build XML structure
	xmlBankData := xmlBank{
		Version:   bank.Version,
		Sections:  make([]xmlSection, 0, len(bank.Sections)),
		Signature: bank.Signature,
	}

	// Convert sections
	for _, section := range bank.Sections {
		xmlSec := xmlSection{
			Name: section.Name,
			Keys: make([]xmlKey, 0, len(section.Keys)),
		}

		// Convert keys
		for _, key := range section.Keys {
			xmlSec.Keys = append(xmlSec.Keys, xmlKey{
				Name:  key.Name,
				Type:  key.Type,
				Value: key.Value,
			})
		}

		xmlBankData.Sections = append(xmlBankData.Sections, xmlSec)
	}

	// Marshal to XML with indentation
	output, err := xml.MarshalIndent(xmlBankData, "", "\t")
	if err != nil {
		return nil, fmt.Errorf("failed to marshal bank to XML: %w", err)
	}

	// Add XML declaration
	xmlDeclaration := `<?xml version="1.0" encoding="utf-8"?>` + "\n"
	result := xmlDeclaration + string(output)

	// Clean up empty signature elements
	if bank.Signature == "" {
		result = strings.Replace(result, "\n\t<Signature></Signature>", "", 1)
		result = strings.Replace(result, "<Signature></Signature>\n", "", 1)
	}

	return []byte(result), nil
}
