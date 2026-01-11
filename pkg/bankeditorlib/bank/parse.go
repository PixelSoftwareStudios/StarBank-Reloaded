package bank

import (
	"encoding/xml"
	"fmt"
)

// xmlBank represents the XML structure of a bank file
type xmlBank struct {
	XMLName   xml.Name     `xml:"Bank"`
	Version   string       `xml:"version,attr"`
	Sections  []xmlSection `xml:"Section"`
	Signature string       `xml:"Signature"`
}

// xmlSection represents a section in the XML
type xmlSection struct {
	Name string   `xml:"name,attr"`
	Keys []xmlKey `xml:"Key"`
}

// xmlKey represents a key in the XML
type xmlKey struct {
	Name  string `xml:"name,attr"`
	Type  string `xml:"type,attr"`
	Value string `xml:",chardata"`
}

// ParseBank parses SC2Bank XML data into a Bank model
// Preserves section and key order as they appear in the XML
func ParseBank(xmlData []byte) (*Bank, error) {
	var xmlBankData xmlBank

	// Parse XML
	if err := xml.Unmarshal(xmlData, &xmlBankData); err != nil {
		return nil, fmt.Errorf("failed to parse bank XML: %w", err)
	}

	// Create new bank
	bank := NewBank()

	// Set version (default to "1" if not specified)
	if xmlBankData.Version != "" {
		bank.Version = xmlBankData.Version
	}

	// Set signature if present
	bank.Signature = xmlBankData.Signature

	// Parse sections in order
	for _, xmlSec := range xmlBankData.Sections {
		section := bank.AddSection(xmlSec.Name)

		// Parse keys in order
		for _, xmlK := range xmlSec.Keys {
			section.AddKey(xmlK.Name, xmlK.Type, xmlK.Value)
		}
	}

	return bank, nil
}
