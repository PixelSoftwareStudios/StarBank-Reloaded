package editor

import (
	"fmt"
	"os"
	"path/filepath"

	"starbank/pkg/bankeditorlib/bank"
	"starbank/pkg/bankeditorlib/util"
)

// SaveMode controls how a bank is persisted to disk.
type SaveMode int

const (
	// SaveModeOverwrite replaces the existing file in-place using an atomic rename.
	SaveModeOverwrite SaveMode = iota
	// SaveModeBackup writes a .bak copy of the current file before overwriting.
	SaveModeBackup
)

// Editor manages the lifecycle of a single open bank document.
type Editor struct {
	filePath    string
	originalXML []byte
	bank        *bank.Bank
	dirty       bool
}

// NewEditor returns an empty editor with no bank loaded.
func NewEditor() *Editor {
	return &Editor{bank: bank.NewBank()}
}

// Open reads and parses a .SC2Bank file, then populates BankInfo metadata
// derived from the file path and filesystem stats.
func (e *Editor) Open(path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("reading bank file: %w", err)
	}

	b, err := bank.ParseBank(data)
	if err != nil {
		return fmt.Errorf("parsing bank: %w", err)
	}

	if info, statErr := os.Stat(path); statErr == nil {
		b.BankInfo.DateModified = info.ModTime()
	}
	b.BankInfo.BankPath = path
	b.BankInfo.Name = bankNameFromPath(path)
	b.BankInfo.AuthorID = authorIDFromPath(path)
	b.BankInfo.PlayerID = playerIDFromPath(path)

	e.filePath = path
	e.originalXML = data
	e.bank = b
	e.dirty = false
	return nil
}

// GetBank returns the current bank model for reads and edits.
// Call MarkDirty after making changes.
func (e *Editor) GetBank() *bank.Bank {
	return e.bank
}

// MarkDirty records that the bank has unsaved changes.
func (e *Editor) MarkDirty() {
	e.dirty = true
}

// IsDirty reports whether there are unsaved changes.
func (e *Editor) IsDirty() bool {
	return e.dirty
}

// GetFilePath returns the current file path, or empty string if no file is open.
func (e *Editor) GetFilePath() string {
	return e.filePath
}

// Reset re-parses the original source XML, discarding all unsaved edits.
func (e *Editor) Reset() error {
	if len(e.originalXML) == 0 {
		return fmt.Errorf("no original data to reset to")
	}
	b, err := bank.ParseBank(e.originalXML)
	if err != nil {
		return fmt.Errorf("resetting bank: %w", err)
	}
	b.BankInfo.BankPath = e.filePath
	b.BankInfo.Name = bankNameFromPath(e.filePath)
	b.BankInfo.AuthorID = authorIDFromPath(e.filePath)
	b.BankInfo.PlayerID = playerIDFromPath(e.filePath)

	e.bank = b
	e.dirty = false
	return nil
}

// Save serializes the bank and writes it to the current file path.
// If autoResign is true the bank signature is recalculated before writing.
// SaveModeBackup writes a .bak copy of the original before overwriting.
func (e *Editor) Save(mode SaveMode, autoResign bool) error {
	if e.filePath == "" {
		return fmt.Errorf("no file path set; use SaveAs for a new file")
	}

	if autoResign {
		if err := bank.UpdateSignature(e.bank); err != nil {
			return fmt.Errorf("re-signing bank: %w", err)
		}
	}

	xmlData, err := bank.WriteBank(e.bank)
	if err != nil {
		return fmt.Errorf("serializing bank: %w", err)
	}

	if mode == SaveModeBackup {
		if writeErr := os.WriteFile(e.filePath+".bak", e.originalXML, 0644); writeErr != nil {
			return fmt.Errorf("writing backup: %w", writeErr)
		}
	}

	if err := writeFileAtomic(e.filePath, xmlData); err != nil {
		return fmt.Errorf("writing bank file: %w", err)
	}

	e.originalXML = xmlData
	e.dirty = false
	return nil
}

// SaveAs serializes the bank to a new path and updates the editor to track
// the new file. If autoResign is true the signature is recalculated first.
func (e *Editor) SaveAs(newPath string, autoResign bool) error {
	if newPath == "" {
		return fmt.Errorf("path cannot be empty")
	}

	if autoResign {
		if err := bank.UpdateSignature(e.bank); err != nil {
			return fmt.Errorf("re-signing bank: %w", err)
		}
	}

	xmlData, err := bank.WriteBank(e.bank)
	if err != nil {
		return fmt.Errorf("serializing bank: %w", err)
	}

	if err := writeFileAtomic(newPath, xmlData); err != nil {
		return fmt.Errorf("writing bank file: %w", err)
	}

	e.filePath = newPath
	e.originalXML = xmlData
	e.dirty = false
	e.bank.BankInfo.BankPath = newPath
	e.bank.BankInfo.Name = bankNameFromPath(newPath)
	e.bank.BankInfo.AuthorID = authorIDFromPath(newPath)
	e.bank.BankInfo.PlayerID = playerIDFromPath(newPath)
	return nil
}

// LoadXML parses raw SC2Bank XML and replaces the current bank model,
// preserving BankInfo metadata (path, IDs, name). This is used by the Raw
// View to apply hand-edited XML without going through the file system.
func (e *Editor) LoadXML(data []byte) error {
	b, err := bank.ParseBank(data)
	if err != nil {
		return fmt.Errorf("parsing XML: %w", err)
	}
	// Preserve existing BankInfo so path and IDs are not lost.
	*b.BankInfo = *e.bank.BankInfo
	e.bank = b
	e.dirty = true
	return nil
}

// CopyXML returns the current bank serialized as XML without modifying the
// bank or its dirty state.
func (e *Editor) CopyXML() (string, error) {
	data, err := bank.WriteBank(e.bank)
	if err != nil {
		return "", fmt.Errorf("serializing bank: %w", err)
	}
	return string(data), nil
}

// writeFileAtomic writes data to path via a temp file + rename so the target
// file is never partially written.
func writeFileAtomic(path string, data []byte) error {
	dir := filepath.Dir(path)

	tmp, err := os.CreateTemp(dir, ".sc2bank-*.tmp")
	if err != nil {
		return fmt.Errorf("creating temp file: %w", err)
	}
	tmpPath := tmp.Name()

	ok := false
	defer func() {
		if !ok {
			os.Remove(tmpPath)
		}
	}()

	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return fmt.Errorf("writing temp file: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return fmt.Errorf("syncing temp file: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("closing temp file: %w", err)
	}

	// Preserve original file permissions when overwriting an existing file.
	if info, statErr := os.Stat(path); statErr == nil {
		_ = os.Chmod(tmpPath, info.Mode())
	}

	if err := os.Rename(tmpPath, path); err != nil {
		return fmt.Errorf("replacing file: %w", err)
	}

	ok = true
	return nil
}

// bankNameFromPath returns the filename without its extension.
func bankNameFromPath(path string) string {
	base := filepath.Base(path)
	ext := filepath.Ext(base)
	return base[:len(base)-len(ext)]
}

// authorIDFromPath extracts the author ID from a standard SC2Bank path:
//
//	.../Banks/<authorID>/bankfile.SC2Bank
func authorIDFromPath(path string) string {
	candidate := filepath.Base(filepath.Dir(path))
	if util.IsValidAccountID(candidate) {
		return candidate
	}
	return ""
}

// playerIDFromPath extracts the player ID from a standard SC2Bank path:
//
//	.../Accounts/<accountNum>/<playerID>/Banks/<authorID>/bankfile.SC2Bank
func playerIDFromPath(path string) string {
	dir := filepath.Dir(path) // .../Banks/<authorID>
	dir = filepath.Dir(dir)   // .../Banks
	dir = filepath.Dir(dir)   // .../<playerID>
	candidate := filepath.Base(dir)
	if util.IsValidAccountID(candidate) {
		return candidate
	}
	return ""
}
