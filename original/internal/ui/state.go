package ui

import (
	"starbank/internal/editor"
	"starbank/internal/maps"
	"starbank/internal/settings"
)

// AppState holds runtime application state — the current open bank, discovered
// maps, and a reference to the loaded settings.
type AppState struct {
	Editor      *editor.Editor
	SC2Root     string
	BankFolders []string
	AllMaps     []maps.MapItem
	CurrentMap  *maps.MapItem

	// PathLoadError is non-nil when SC2 path detection failed entirely (as
	// opposed to succeeding but finding zero bank files).
	PathLoadError error

	Settings *settings.Settings
}

// NewAppState creates a fresh AppState, loading persisted settings from disk.
func NewAppState() *AppState {
	return &AppState{
		Editor:   editor.NewEditor(),
		Settings: settings.Load(),
	}
}
