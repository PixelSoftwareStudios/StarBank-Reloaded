package bank

import "time"

// Key type constants as used in SC2Bank <Value type="..."/> attributes.
const (
	KeyTypeFixed  = "fixed"
	KeyTypeFlag   = "flag"
	KeyTypeInt    = "int"
	KeyTypePoint  = "point"
	KeyTypeString = "string"
	KeyTypeText   = "text"
	KeyTypeUnit   = "unit"
)

// BankInfo holds metadata about a bank file path and ownership.
// It does not contain data from inside the bank file itself.
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

// Key represents a single key-value pair within a Section.
type Key struct {
	Name    string
	Type    string
	Value   string
	Section *Section // back-reference to parent
}

func (k *Key) String() string {
	return k.Name
}

// Section represents a named group of Keys within a Bank.
type Section struct {
	Name string
	Keys []*Key
}

func (s *Section) String() string {
	return s.Name
}

// Bank represents a complete parsed .SC2Bank file.
type Bank struct {
	BankInfo  *BankInfo
	Sections  []*Section
	Signature string
	Version   string
}

// NewBank returns an empty Bank with sensible defaults.
func NewBank() *Bank {
	return &Bank{
		BankInfo: &BankInfo{},
		Sections: []*Section{},
		Version:  "1",
	}
}

// AddSection returns the existing Section with the given name, or creates and
// appends a new one.
func (b *Bank) AddSection(name string) *Section {
	if existing := b.FindSection(name); existing != nil {
		return existing
	}
	section := &Section{Name: name, Keys: []*Key{}}
	b.Sections = append(b.Sections, section)
	return section
}

// FindSection returns the Section with the given name, or nil.
func (b *Bank) FindSection(name string) *Section {
	for _, s := range b.Sections {
		if s.Name == name {
			return s
		}
	}
	return nil
}

// AddKey returns the existing Key with the given name (updating its type and
// value), or creates and appends a new one with a back-reference to s.
func (s *Section) AddKey(name, keyType, value string) *Key {
	if existing := s.FindKey(name); existing != nil {
		existing.Type = keyType
		existing.Value = value
		return existing
	}
	key := &Key{Name: name, Type: keyType, Value: value, Section: s}
	s.Keys = append(s.Keys, key)
	return key
}

// FindKey returns the Key with the given name, or nil.
func (s *Section) FindKey(name string) *Key {
	for _, k := range s.Keys {
		if k.Name == name {
			return k
		}
	}
	return nil
}
