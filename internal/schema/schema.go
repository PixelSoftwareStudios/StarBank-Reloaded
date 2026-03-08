// Package schema provides per-map Simple View definitions that map cryptic
// SC2Bank keys to human-readable labels and editor field types.
package schema

// FieldType determines how a bank key is displayed and edited in Simple View.
type FieldType string

const (
	// FieldTypeNumber renders a text entry that strips commas on write.
	FieldTypeNumber FieldType = "number"
	// FieldTypeCheckbox renders a checkbox that stores "1" or "0".
	FieldTypeCheckbox FieldType = "checkbox"
	// FieldTypeString renders a plain text entry.
	FieldTypeString FieldType = "string"
)

// FieldDef maps a single SC2Bank key to a display label and editor widget.
type FieldDef struct {
	Label   string    `yaml:"label"`
	Section string    `yaml:"section"`
	Key     string    `yaml:"key"`
	Type    FieldType `yaml:"type"`
}

// GroupDef is a named collection of FieldDefs rendered as a labeled box.
type GroupDef struct {
	Label  string     `yaml:"label"`
	Fields []FieldDef `yaml:"fields"`
}

// MapSchema is the complete Simple View definition for one map.
// MapName must match the .SC2Bank filename (without extension) exactly.
type MapSchema struct {
	MapName string     `yaml:"map"`
	Groups  []GroupDef `yaml:"groups"`
}

// Lookup returns the MapSchema for the given bank filename (case-insensitive),
// or nil if no schema is registered for that map.
func Lookup(bankName string) *MapSchema {
	return registry.lookup(bankName)
}
