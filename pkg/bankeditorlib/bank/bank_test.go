package bank

import (
	"strings"
	"testing"
)

// realBankXML is a minimal SC2Bank file in the real on-disk format.
// Keys use <Value type="val"/> child elements, not attributes on <Key>.
// The signature uses <Signature value="..."/>.
const realBankXML = `<?xml version="1.0" encoding="utf-8"?>
<Bank version="1">
	<Section name="xp">
		<Key name="xp">
			<Value int="10000000943"/>
		</Key>
	</Section>
	<Section name="stats">
		<Key name="level">
			<Value int="50"/>
		</Key>
		<Key name="gold">
			<Value string="1000000"/>
		</Key>
	</Section>
</Bank>`

func TestParseAndWriteRoundTrip(t *testing.T) {
	b, err := ParseBank([]byte(realBankXML))
	if err != nil {
		t.Fatalf("ParseBank: %v", err)
	}

	if b.Version != "1" {
		t.Errorf("version: got %q, want %q", b.Version, "1")
	}
	if len(b.Sections) != 2 {
		t.Fatalf("sections: got %d, want 2", len(b.Sections))
	}

	xpSec := b.FindSection("xp")
	if xpSec == nil {
		t.Fatal("section 'xp' not found")
	}
	xpKey := xpSec.FindKey("xp")
	if xpKey == nil {
		t.Fatal("key 'xp' not found")
	}
	if xpKey.Type != KeyTypeInt {
		t.Errorf("key type: got %q, want %q", xpKey.Type, KeyTypeInt)
	}
	if xpKey.Value != "10000000943" {
		t.Errorf("key value: got %q, want %q", xpKey.Value, "10000000943")
	}
	if xpKey.Section != xpSec {
		t.Error("key back-reference to section is incorrect")
	}

	// Write back and verify the output contains expected SC2Bank elements.
	out, err := WriteBank(b)
	if err != nil {
		t.Fatalf("WriteBank: %v", err)
	}
	outStr := string(out)

	mustContain := []string{
		`<?xml version="1.0" encoding="utf-8"?>`,
		`<Bank version="1">`,
		`<Section name="xp">`,
		`<Key name="xp">`,
		`<Value int="10000000943">`,
		`<Key name="level">`,
		`<Value int="50">`,
		`<Key name="gold">`,
		`<Value string="1000000">`,
	}
	for _, want := range mustContain {
		if !strings.Contains(outStr, want) {
			t.Errorf("output missing: %s\nfull output:\n%s", want, outStr)
		}
	}
}

func TestParseWithSignature(t *testing.T) {
	const xml = `<?xml version="1.0" encoding="utf-8"?>
<Bank version="1">
	<Section name="test">
		<Key name="value">
			<Value int="123"/>
		</Key>
	</Section>
	<Signature value="ABC123DEF456"/>
</Bank>`

	b, err := ParseBank([]byte(xml))
	if err != nil {
		t.Fatalf("ParseBank: %v", err)
	}
	if b.Signature != "ABC123DEF456" {
		t.Errorf("signature: got %q, want %q", b.Signature, "ABC123DEF456")
	}

	out, err := WriteBank(b)
	if err != nil {
		t.Fatalf("WriteBank: %v", err)
	}
	if !strings.Contains(string(out), `<Signature value="ABC123DEF456">`) {
		t.Errorf("signature not preserved in output:\n%s", string(out))
	}
}

func TestAddSectionAndKey(t *testing.T) {
	b := NewBank()

	sec := b.AddSection("inventory")
	if sec == nil {
		t.Fatal("AddSection returned nil")
	}

	key := sec.AddKey("sword", KeyTypeInt, "5")
	if key == nil {
		t.Fatal("AddKey returned nil")
	}
	if key.Name != "sword" || key.Type != KeyTypeInt || key.Value != "5" {
		t.Errorf("key fields: got {%q %q %q}, want {%q %q %q}",
			key.Name, key.Type, key.Value, "sword", KeyTypeInt, "5")
	}
	if key.Section != sec {
		t.Error("key back-reference to section is incorrect")
	}
	if sec.FindKey("sword") != key {
		t.Error("FindKey did not return the same key instance")
	}

	// Duplicate section returns the existing one.
	if b.AddSection("inventory") != sec {
		t.Error("AddSection should return existing section for duplicate name")
	}

	// Duplicate key updates type and value, returns same pointer.
	updated := sec.AddKey("sword", KeyTypeString, "10")
	if updated != key {
		t.Error("AddKey should return existing key for duplicate name")
	}
	if key.Value != "10" || key.Type != KeyTypeString {
		t.Errorf("AddKey did not update existing key: got {%q %q}", key.Type, key.Value)
	}
}

func TestParseMissingSection(t *testing.T) {
	b, err := ParseBank([]byte(realBankXML))
	if err != nil {
		t.Fatalf("ParseBank: %v", err)
	}
	if b.FindSection("nonexistent") != nil {
		t.Error("FindSection should return nil for unknown section")
	}
}
