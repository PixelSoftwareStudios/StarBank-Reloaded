package editor

import (
	"fmt"
	"os"
	"path/filepath"
	"starbank/pkg/bankeditorlib/bank"
)

// SaveMode defines how a bank file should be saved
type SaveMode int

const (
	// SaveModeOverwrite replaces the existing file
	SaveModeOverwrite SaveMode = iota
	// SaveModeBackup creates a .bak file before saving
	SaveModeBackup
	// SaveModeAs saves to a new location (path provided separately)
	SaveModeAs
)

// Editor manages the lifecycle of a bank document
type Editor struct {
	filePath    string
	originalXML []byte
	bank        *bank.Bank
	dirty       bool
}

// NewEditor creates a new empty editor
func NewEditor() *Editor {
	return &Editor{
		bank:  bank.NewBank(),
		dirty: false,
	}
}

// Open loads a bank file from disk and parses it
func (e *Editor) Open(path string) error {
	// Read file
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("failed to read bank file: %w", err)
	}

	// Parse bank
	parsedBank, err := bank.ParseBank(data)
	if err != nil {
		return fmt.Errorf("failed to parse bank file: %w", err)
	}

	// Update BankInfo metadata
	fileInfo, err := os.Stat(path)
	if err == nil {
		parsedBank.BankInfo.DateModified = fileInfo.ModTime()
		parsedBank.BankInfo.BankPath = path
		parsedBank.BankInfo.Name = filepath.Base(path)
	}

	// Store state
	e.filePath = path
	e.originalXML = data
	e.bank = parsedBank
	e.dirty = false

	return nil
}

// GetBank returns the current bank model for UI to read/edit
func (e *Editor) GetBank() *bank.Bank {
	return e.bank
}

// MarkDirty sets the dirty flag to indicate unsaved changes
func (e *Editor) MarkDirty() {
	e.dirty = true
}

// IsDirty returns whether there are unsaved changes
func (e *Editor) IsDirty() bool {
	return e.dirty
}

// Reset reverts the bank to the original XML state
func (e *Editor) Reset() error {
	if len(e.originalXML) == 0 {
		return fmt.Errorf("no original data to reset to")
	}

	// Re-parse original XML
	parsedBank, err := bank.ParseBank(e.originalXML)
	if err != nil {
		return fmt.Errorf("failed to reset bank: %w", err)
	}

	// Restore metadata
	if e.filePath != "" {
		parsedBank.BankInfo.BankPath = e.filePath
		parsedBank.BankInfo.Name = filepath.Base(e.filePath)
	}

	e.bank = parsedBank
	e.dirty = false

	return nil
}

// Save writes the current bank to disk according to the specified mode
// autoResign indicates whether to re-sign after saving (not implemented yet)
func (e *Editor) Save(mode SaveMode, autoResign bool) error {
	// Generate XML from current bank
	xmlData, err := bank.WriteBank(e.bank)
	if err != nil {
		return fmt.Errorf("failed to generate XML: %w", err)
	}

	// Future: if autoResign is true, sign the data here
	// For now, this is a placeholder for future signing implementation

	targetPath := e.filePath

	switch mode {
	case SaveModeBackup:
		// Create backup of original file
		if e.filePath != "" {
			backupPath := e.filePath + ".bak"
			if err := os.WriteFile(backupPath, e.originalXML, 0644); err != nil {
				return fmt.Errorf("failed to create backup: %w", err)
			}
		}
		fallthrough

	case SaveModeOverwrite:
		if targetPath == "" {
			return fmt.Errorf("no file path set for save")
		}
		if err := os.WriteFile(targetPath, xmlData, 0644); err != nil {
			return fmt.Errorf("failed to write bank file: %w", err)
		}

	case SaveModeAs:
		// SaveModeAs requires the path to be set externally before calling
		// This is handled by SaveAs method
		return fmt.Errorf("use SaveAs method for SaveModeAs mode")
	}

	// Update state on successful save
	e.originalXML = xmlData
	e.dirty = false

	return nil
}

// SaveAs saves the bank to a new file path
func (e *Editor) SaveAs(newPath string, autoResign bool) error {
	if newPath == "" {
		return fmt.Errorf("new path cannot be empty")
	}

	// Generate XML from current bank
	xmlData, err := bank.WriteBank(e.bank)
	if err != nil {
		return fmt.Errorf("failed to generate XML: %w", err)
	}

	// Future: if autoResign is true, sign the data here

	// Write to new path
	if err := os.WriteFile(newPath, xmlData, 0644); err != nil {
		return fmt.Errorf("failed to write bank file: %w", err)
	}

	// Update editor state to new path
	e.filePath = newPath
	e.originalXML = xmlData
	e.dirty = false

	// Update bank info
	e.bank.BankInfo.BankPath = newPath
	e.bank.BankInfo.Name = filepath.Base(newPath)

	return nil
}

// CopyXML returns the current bank as an XML string for clipboard
func (e *Editor) CopyXML() (string, error) {
	xmlData, err := bank.WriteBank(e.bank)
	if err != nil {
		return "", fmt.Errorf("failed to generate XML: %w", err)
	}
	return string(xmlData), nil
}

// GetFilePath returns the current file path
func (e *Editor) GetFilePath() string {
	return e.filePath
}
