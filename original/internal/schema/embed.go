package schema

import (
	"embed"
	"io/fs"
	"strings"

	"gopkg.in/yaml.v3"
)

//go:embed schemas
var embeddedSchemas embed.FS

// schemaRegistry holds the in-memory map of bank names to MapSchemas.
type schemaRegistry struct {
	schemas map[string]*MapSchema
}

func (r *schemaRegistry) lookup(name string) *MapSchema {
	if r == nil || r.schemas == nil {
		return nil
	}
	return r.schemas[strings.ToLower(name)]
}

// registry is populated by init() from the embedded YAML files.
var registry = &schemaRegistry{schemas: make(map[string]*MapSchema)}

func init() {
	entries, err := fs.ReadDir(embeddedSchemas, "schemas")
	if err != nil {
		return
	}
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".yaml") {
			continue
		}
		data, readErr := embeddedSchemas.ReadFile("schemas/" + entry.Name())
		if readErr != nil {
			continue
		}
		var s MapSchema
		if unmarshalErr := yaml.Unmarshal(data, &s); unmarshalErr != nil {
			continue
		}
		if s.MapName != "" {
			registry.schemas[strings.ToLower(s.MapName)] = &s
		}
	}
}
