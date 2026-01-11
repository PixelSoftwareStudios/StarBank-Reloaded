package bank

import (
	"strings"
	"testing"
)

func TestParseAndWriteRoundTrip(t *testing.T) {
	// Sample SC2Bank XML
	xmlInput := `<?xml version="1.0" encoding="utf-8"?>
<Bank version="1">
	<Section name="xp">
		<Key name="xp" type="int">10000000943</Key>
	</Section>
	<Section name="stats">
		<Key name="level" type="int">50</Key>
		<Key name="gold" type="string">1000000</Key>
	</Section>
</Bank>`

	// Parse the XML
	bank, err := ParseBank([]byte(xmlInput))
	if err != nil {
		t.Fatalf("Failed to parse bank: %v", err)
	}

	// Verify parsing
	if bank.Version != "1" {
		t.Errorf("Expected version '1', got '%s'", bank.Version)
	}

	if len(bank.Sections) != 2 {
		t.Fatalf("Expected 2 sections, got %d", len(bank.Sections))
	}

	// Verify first section
	xpSection := bank.FindSection("xp")
	if xpSection == nil {
		t.Fatal("Section 'xp' not found")
	}
	if len(xpSection.Keys) != 1 {
		t.Errorf("Expected 1 key in xp section, got %d", len(xpSection.Keys))
	}

	xpKey := xpSection.FindKey("xp")
	if xpKey == nil {
		t.Fatal("Key 'xp' not found")
	}
	if xpKey.Type != "int" {
		t.Errorf("Expected type 'int', got '%s'", xpKey.Type)
	}
	if xpKey.Value != "10000000943" {
		t.Errorf("Expected value '10000000943', got '%s'", xpKey.Value)
	}

	// Verify back-reference
	if xpKey.Section != xpSection {
		t.Error("Key back-reference to section is incorrect")
	}

	// Write back to XML
	xmlOutput, err := WriteBank(bank)
	if err != nil {
		t.Fatalf("Failed to write bank: %v", err)
	}

	// Verify output contains expected elements
	outputStr := string(xmlOutput)
	if !strings.Contains(outputStr, `<?xml version="1.0" encoding="utf-8"?>`) {
		t.Error("Output missing XML declaration")
	}
	if !strings.Contains(outputStr, `<Bank version="1">`) {
		t.Error("Output missing Bank element with version")
	}
	if !strings.Contains(outputStr, `<Section name="xp">`) {
		t.Error("Output missing xp section")
	}
	if !strings.Contains(outputStr, `<Key name="xp" type="int">10000000943</Key>`) {
		t.Error("Output missing xp key")
	}
}

func TestAddSectionAndKey(t *testing.T) {
	bank := NewBank()

	// Add section
	section := bank.AddSection("inventory")
	if section == nil {
		t.Fatal("Failed to add section")
	}

	// Add key
	key := section.AddKey("sword", "int", "5")
	if key == nil {
		t.Fatal("Failed to add key")
	}

	// Verify key properties
	if key.Name != "sword" {
		t.Errorf("Expected key name 'sword', got '%s'", key.Name)
	}
	if key.Type != "int" {
		t.Errorf("Expected key type 'int', got '%s'", key.Type)
	}
	if key.Value != "5" {
		t.Errorf("Expected key value '5', got '%s'", key.Value)
	}
	if key.Section != section {
		t.Error("Key back-reference to section is incorrect")
	}

	// Verify section contains key
	foundKey := section.FindKey("sword")
	if foundKey != key {
		t.Error("FindKey did not return the same key instance")
	}

	// Test adding duplicate section (should return existing)
	section2 := bank.AddSection("inventory")
	if section2 != section {
		t.Error("AddSection should return existing section for duplicate name")
	}

	// Test updating existing key
	key2 := section.AddKey("sword", "string", "10")
	if key2 != key {
		t.Error("AddKey should return existing key for duplicate name")
	}
	if key.Value != "10" {
		t.Errorf("Expected updated value '10', got '%s'", key.Value)
	}
	if key.Type != "string" {
		t.Errorf("Expected updated type 'string', got '%s'", key.Type)
	}
}

func TestParseWithSignature(t *testing.T) {
	xmlInput := `<?xml version="1.0" encoding="utf-8"?>
<Bank version="1">
	<Section name="test">
		<Key name="value" type="int">123</Key>
	</Section>
	<Signature>ABC123DEF456</Signature>
</Bank>`

	bank, err := ParseBank([]byte(xmlInput))
	if err != nil {
		t.Fatalf("Failed to parse bank with signature: %v", err)
	}

	if bank.Signature != "ABC123DEF456" {
		t.Errorf("Expected signature 'ABC123DEF456', got '%s'", bank.Signature)
	}

	// Write back and verify signature is preserved
	xmlOutput, err := WriteBank(bank)
	if err != nil {
		t.Fatalf("Failed to write bank: %v", err)
	}

	if !strings.Contains(string(xmlOutput), "<Signature>ABC123DEF456</Signature>") {
		t.Error("Signature not preserved in output")
	}
}
