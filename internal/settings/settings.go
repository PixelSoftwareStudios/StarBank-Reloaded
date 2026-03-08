// Package settings manages user preferences for StarBank Reloaded.
// Settings are persisted as JSON in the OS user config directory:
//   - macOS:   ~/Library/Application Support/StarBank-Reloaded/settings.json
//   - Windows: %APPDATA%\StarBank-Reloaded\settings.json
//   - Linux:   ~/.config/StarBank-Reloaded/settings.json
package settings

import (
	"encoding/json"
	"os"
	"path/filepath"
)

const appName = "StarBank-Reloaded"

// Settings holds all user-configurable preferences.
// New fields can be added freely; unknown keys are ignored on load.
type Settings struct {
	// Map list filters
	HideBlizzardMaps     bool `json:"hide_blizzard_maps"`
	HideMapsWithoutBanks bool `json:"hide_maps_without_banks"`

	// Editor behaviour
	AutoResignOnSave bool `json:"auto_resign_on_save"`

	// SC2 root path override — empty means auto-detect on startup.
	SC2RootOverride string `json:"sc2_root_override,omitempty"`
}

// Default returns a Settings instance populated with sensible defaults.
func Default() *Settings {
	return &Settings{
		HideBlizzardMaps:     true,
		HideMapsWithoutBanks: true,
		AutoResignOnSave:     true,
	}
}

// Load reads settings from disk, falling back to defaults for any unreadable
// or missing file. Partial JSON is handled gracefully — only present keys are
// applied; the rest keep their default values.
func Load() *Settings {
	s := Default()
	path, err := configFilePath()
	if err != nil {
		return s
	}
	data, err := os.ReadFile(path)
	if err != nil {
		// File not found on first run — not an error.
		return s
	}
	_ = json.Unmarshal(data, s) // silently ignore partial/corrupt JSON
	return s
}

// Save writes settings to disk. The config directory is created if needed.
func (s *Settings) Save() error {
	path, err := configFilePath()
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0644)
}

// configFilePath returns the absolute path to the settings JSON file.
func configFilePath() (string, error) {
	dir, err := os.UserConfigDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, appName, "settings.json"), nil
}
