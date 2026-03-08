// Package maps handles discovery of .SC2Bank files from the SC2 accounts tree.
package maps

import (
	"os"
	"path/filepath"
	"slices"
	"strings"

	"starbank/pkg/bankeditorlib/util"
)

// MapItem represents a single .SC2Bank file discovered on disk.
// The bank filename (without extension) is used as the display name since
// map authors typically name their bank after their map.
type MapItem struct {
	// Name is the bank filename without extension, e.g. "PlayerData".
	Name string
	// AuthorID is the 1-S2-X-XXXXX folder that contains the bank file.
	AuthorID string
	// BankPath is the absolute path to the .SC2Bank file.
	BankPath string
	// HasBankFiles is always true for items returned by DiscoverMaps since
	// discovery only yields actual .SC2Bank files.
	HasBankFiles bool
	// IsBlizzardMap is reserved for future use; always false currently.
	IsBlizzardMap bool
}

// DiscoverMaps walks one or more Banks folder paths and returns all .SC2Bank
// files found, sorted by name. Invalid or inaccessible folders are silently
// skipped.
func DiscoverMaps(bankFolders []string) []MapItem {
	var items []MapItem
	for _, folder := range bankFolders {
		items = append(items, discoverInFolder(folder)...)
	}
	slices.SortStableFunc(items, func(a, b MapItem) int {
		return strings.Compare(strings.ToLower(a.Name), strings.ToLower(b.Name))
	})
	return items
}

// discoverInFolder walks a single Banks folder:
//
//	Banks/
//	  <authorID>/
//	    BankName.SC2Bank
func discoverInFolder(banksFolder string) []MapItem {
	authorDirs, err := os.ReadDir(banksFolder)
	if err != nil {
		return nil
	}

	var items []MapItem
	for _, authorEntry := range authorDirs {
		if !authorEntry.IsDir() {
			continue
		}
		authorID := authorEntry.Name()
		if !util.IsValidAccountID(authorID) {
			continue
		}
		authorPath := filepath.Join(banksFolder, authorID)

		bankFiles, err := os.ReadDir(authorPath)
		if err != nil {
			continue
		}
		for _, bankFile := range bankFiles {
			if bankFile.IsDir() {
				continue
			}
			if !strings.EqualFold(filepath.Ext(bankFile.Name()), ".sc2bank") {
				continue
			}
			name := strings.TrimSuffix(bankFile.Name(), filepath.Ext(bankFile.Name()))
			items = append(items, MapItem{
				Name:          name,
				AuthorID:      authorID,
				BankPath:      filepath.Join(authorPath, bankFile.Name()),
				HasBankFiles:  true,
				IsBlizzardMap: false,
			})
		}
	}
	return items
}
