package ui

import (
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	"starbank/internal/maps"
)

// BuildSidebar creates the left panel with the map list and quick options.
// It returns the CanvasObject and a reloadMaps callback that refreshes the
// displayed list when state.AllMaps has been updated externally.
func BuildSidebar(
	state *AppState,
	onMapSelected func(maps.MapItem),
	onFilterChanged func(),
) (fyne.CanvasObject, func()) {

	search := widget.NewEntry()
	search.SetPlaceHolder("Search maps…")

	hideBlizzard := widget.NewCheck("Hide Blizzard maps", func(v bool) {
		state.Settings.HideBlizzardMaps = v
		onFilterChanged()
	})
	hideBlizzard.SetChecked(state.Settings.HideBlizzardMaps)

	hideNoBanks := widget.NewCheck("Hide maps without bank files", func(v bool) {
		state.Settings.HideMapsWithoutBanks = v
		onFilterChanged()
	})
	hideNoBanks.SetChecked(state.Settings.HideMapsWithoutBanks)

	autoResign := widget.NewCheck("Auto re-sign banks on save", func(v bool) {
		state.Settings.AutoResignOnSave = v
		_ = state.Settings.Save()
	})
	autoResign.SetChecked(state.Settings.AutoResignOnSave)

	filtered := filterMaps(state, "")

	mapList := widget.NewList(
		func() int { return len(filtered) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, obj fyne.CanvasObject) {
			obj.(*widget.Label).SetText(filtered[i].Name)
		},
	)
	mapList.OnSelected = func(id widget.ListItemID) {
		if id < 0 || id >= len(filtered) {
			return
		}
		onMapSelected(filtered[id])
	}

	listScroll := container.NewVScroll(mapList)
	listScroll.SetMinSize(fyne.NewSize(0, 240))

	// emptyState is shown in place of the list when the filtered slice is empty.
	emptyLabel := widget.NewLabelWithStyle(
		"",
		fyne.TextAlignCenter,
		fyne.TextStyle{Italic: true},
	)
	emptyLabel.Wrapping = fyne.TextWrapWord
	emptyState := container.NewCenter(emptyLabel)

	// listArea switches between the list and the empty-state label.
	listArea := container.NewMax()

	updateDisplay := func() {
		listArea.Objects = nil
		if len(filtered) == 0 {
			switch {
			case state.PathLoadError != nil:
				emptyLabel.SetText("Could not load bank files:\n" +
					state.PathLoadError.Error() +
					"\n\nSelect your SC2 folder\nvia ⚙ Settings.")
			case len(state.AllMaps) == 0:
				emptyLabel.SetText("No bank files found.\nSelect your SC2 folder\nvia ⚙ Settings.")
			default:
				emptyLabel.SetText("No banks match\nyour search.")
			}
			listArea.Add(emptyState)
		} else {
			listArea.Add(listScroll)
		}
		listArea.Refresh()
	}
	updateDisplay()

	refilter := func() {
		filtered = filterMaps(state, search.Text)
		mapList.Refresh()
		updateDisplay()
	}

	search.OnChanged = func(string) {
		refilter()
	}

	optionsBox := container.NewVBox(
		widget.NewSeparator(),
		widget.NewLabelWithStyle("Quick Options", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		hideBlizzard,
		hideNoBanks,
		autoResign,
	)

	content := container.NewBorder(
		container.NewVBox(
			widget.NewLabelWithStyle("Map List", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
			widget.NewSeparator(),
			container.NewPadded(search),
		),
		container.NewPadded(optionsBox),
		nil, nil,
		container.NewPadded(listArea),
	)

	return content, refilter
}

// filterMaps applies the current search text and settings filters to
// state.AllMaps and returns the matching subset.
func filterMaps(state *AppState, searchText string) []maps.MapItem {
	q := strings.ToLower(strings.TrimSpace(searchText))
	out := make([]maps.MapItem, 0, len(state.AllMaps))
	for _, m := range state.AllMaps {
		if state.Settings.HideBlizzardMaps && m.IsBlizzardMap {
			continue
		}
		if state.Settings.HideMapsWithoutBanks && !m.HasBankFiles {
			continue
		}
		if q != "" && !strings.Contains(strings.ToLower(m.Name), q) {
			continue
		}
		out = append(out, m)
	}
	return out
}
