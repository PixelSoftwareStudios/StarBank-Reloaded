package bank

import (
	"testing"
)

func TestGenerateChecksum(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "Quick brown fox over lazy dog",
			input:    "The quick brown fox jumps over the lazy dog",
			expected: "2FD4E1C67A2D28FCED849EE1BB76E7391B93EB12",
		},
		{
			name:     "Quick brown fox over lazy cog",
			input:    "The quick brown fox jumps over the lazy cog",
			expected: "DE9F2C7FD25E1B3AFAD3E85A0BD17D9B100DB4B3",
		},
		{
			name:     "Empty string",
			input:    "",
			expected: "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := generateChecksum(tt.input)
			if result != tt.expected {
				t.Errorf("generateChecksum(%q) = %q, want %q", tt.input, result, tt.expected)
			}
		})
	}
}

func TestUpdateSignature(t *testing.T) {
	t.Run("Nil bank returns error", func(t *testing.T) {
		err := UpdateSignature(nil)
		if err == nil {
			t.Error("UpdateSignature(nil) should return an error")
		}
		if err.Error() != "Invalid bank" {
			t.Errorf("Expected error message 'Invalid bank', got %q", err.Error())
		}
	})

	t.Run("Valid bank generates signature", func(t *testing.T) {
		bank := NewBank()
		bank.BankInfo.AuthorID = "1234"
		bank.BankInfo.PlayerID = "5678"
		bank.BankInfo.Name = "TestBank"

		// Add a section and key
		section := bank.AddSection("testSection")
		section.AddKey("testKey", "int", "100")

		err := UpdateSignature(bank)
		if err != nil {
			t.Errorf("UpdateSignature() returned error: %v", err)
		}

		if bank.Signature == "" {
			t.Error("UpdateSignature() did not set a signature")
		}

		// Signature should be 40 characters (SHA1 hex)
		if len(bank.Signature) != 40 {
			t.Errorf("Signature length = %d, want 40", len(bank.Signature))
		}
	})

	// TODO: Add more comprehensive tests for GetSignature functionality
	// - Test signature consistency (same input = same output)
	// - Test signature changes when bank data changes
	// - Test with multiple sections/keys
	// - Test signature format validation
}
