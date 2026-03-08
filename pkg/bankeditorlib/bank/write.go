package bank

import (
	"encoding/xml"
	"fmt"
)

// WriteBank serializes a Bank model to SC2Bank XML bytes.
// The caller is responsible for writing the result to disk.
// Section and key order from the model is preserved exactly.
func WriteBank(b *Bank) ([]byte, error) {
	if b == nil {
		return nil, fmt.Errorf("bank cannot be nil")
	}

	xb := xmlBank{
		Version:  b.Version,
		Sections: make([]xmlSection, 0, len(b.Sections)),
	}

	if b.Signature != "" {
		xb.Signature = &xmlSignature{Value: b.Signature}
	}

	for _, sec := range b.Sections {
		xs := xmlSection{
			Name: sec.Name,
			Keys: make([]xmlKey, 0, len(sec.Keys)),
		}
		for _, k := range sec.Keys {
			xs.Keys = append(xs.Keys, xmlKey{
				Name:  k.Name,
				Value: xmlValue{Type: k.Type, Value: k.Value},
			})
		}
		xb.Sections = append(xb.Sections, xs)
	}

	out, err := xml.MarshalIndent(xb, "", "\t")
	if err != nil {
		return nil, fmt.Errorf("marshaling bank XML: %w", err)
	}

	const header = `<?xml version="1.0" encoding="utf-8"?>` + "\n"
	return append([]byte(header), out...), nil
}
