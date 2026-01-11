package ui

import (
	"starbank/internal/editor"
)

// MapItem represents a StarCraft II map with bank files
type MapItem struct {
	Name          string
	HasBankFiles  bool
	IsBlizzardMap bool
}

// AppState holds the application state and editor
type AppState struct {
	Editor *editor.Editor

	// UI preferences
	HideBlizzardMaps     bool
	HideMapsWithoutBanks bool
	AutoResignOnSave     bool

	// Map discovery
	AllMaps      []MapItem
	FilteredMaps []MapItem

	// Current selection
	CurrentMapName  string
	CurrentBankName string
}

// NewAppState creates a new application state with default values
func NewAppState() *AppState {
	return &AppState{
		Editor:               editor.NewEditor(),
		HideBlizzardMaps:     true,
		HideMapsWithoutBanks: true,
		AutoResignOnSave:     true,
		AllMaps:              getDefaultMaps(),
		FilteredMaps:         []MapItem{},
	}
}

// getDefaultMaps returns the default mock map list
// TODO: Replace with real map discovery from SC2 directory
func getDefaultMaps() []MapItem {
	return []MapItem{
		{"?????? ?????? ???????", true, false},
		{"Custom Hero Defense", true, false},
		{"Entropy TD", true, false},
		{"Lottery Defence", true, false},
		{"Mafia", true, false},
		{"MineralZ and the Thing", false, false},
		{"Penguin Warz", true, false},
		{"Squadron TD Veteran Test", false, false},
		{"Blizzard Example Map", true, true},
	}
}
