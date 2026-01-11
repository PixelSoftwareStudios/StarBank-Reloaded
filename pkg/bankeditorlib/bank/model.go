package bank

import "time"

// BankInfo contains metadata about a bank file
type BankInfo struct {
	BankPath     string
	Name         string
	DateModified time.Time
	PlayerID     string
	AuthorID     string
}

func (bi *BankInfo) String() string {
	return bi.Name
}

// Key represents a key-value pair within a section
type Key struct {
	Name    string
	Type    string
	Value   string
	Section *Section // back-reference to parent section
}

func (k *Key) String() string {
	return k.Name
}

// Section represents a named group of keys
type Section struct {
	Name string
	Keys []*Key
}

func (s *Section) String() string {
	return s.Name
}

// Bank represents a complete SC2Bank file structure
type Bank struct {
	BankInfo  *BankInfo
	Sections  []*Section
	Signature string
	Version   string
}

// NewBank creates a new empty Bank with initialized fields
func NewBank() *Bank {
	return &Bank{
		BankInfo: &BankInfo{},
		Sections: []*Section{},
		Version:  "1",
	}
}

// AddSection adds a new section or returns existing one with the given name
func (b *Bank) AddSection(name string) *Section {
	// Check if section already exists
	if existing := b.FindSection(name); existing != nil {
		return existing
	}

	// Create new section
	section := &Section{
		Name: name,
		Keys: []*Key{},
	}
	b.Sections = append(b.Sections, section)
	return section
}

// FindSection returns the section with the given name, or nil if not found
func (b *Bank) FindSection(name string) *Section {
	for _, section := range b.Sections {
		if section.Name == name {
			return section
		}
	}
	return nil
}

// AddKey adds a new key or updates existing one with the given name
// Sets the back-reference to the parent section
func (s *Section) AddKey(name, keyType, value string) *Key {
	// Check if key already exists
	if existing := s.FindKey(name); existing != nil {
		existing.Type = keyType
		existing.Value = value
		return existing
	}

	// Create new key with back-reference
	key := &Key{
		Name:    name,
		Type:    keyType,
		Value:   value,
		Section: s,
	}
	s.Keys = append(s.Keys, key)
	return key
}

// FindKey returns the key with the given name, or nil if not found
func (s *Section) FindKey(name string) *Key {
	for _, key := range s.Keys {
		if key.Name == name {
			return key
		}
	}
	return nil
}
