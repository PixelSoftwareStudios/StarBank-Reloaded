package ui

import (
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// BuildSidebar creates the left panel with map list and filter options
func BuildSidebar(state *AppState, onMapSelected func(MapItem), onFilterChanged func()) fyne.CanvasObject {
	// Search entry
	search := widget.NewEntry()
	search.SetPlaceHolder("Search maps…")

	// Filter checkboxes
	hideBlizzard := widget.NewCheck("Hide Blizzard maps", func(v bool) {
		state.HideBlizzardMaps = v
		onFilterChanged()
	})
	hideBlizzard.SetChecked(state.HideBlizzardMaps)

	hideNoBanks := widget.NewCheck("Hide maps without bank files", func(v bool) {
		state.HideMapsWithoutBanks = v
		onFilterChanged()
	})
	hideNoBanks.SetChecked(state.HideMapsWithoutBanks)

	autoResign := widget.NewCheck("Auto re-sign banks on save", func(v bool) {
		state.AutoResignOnSave = v
	})
	autoResign.SetChecked(state.AutoResignOnSave)

	// Filter maps based on search and preferences
	filterMaps := func() []MapItem {
		q := strings.ToLower(strings.TrimSpace(search.Text))
		out := []MapItem{}
		for _, m := range state.AllMaps {
			if state.HideBlizzardMaps && m.IsBlizzardMap {
				continue
			}
			if state.HideMapsWithoutBanks && !m.HasBankFiles {
				continue
			}
			if q != "" && !strings.Contains(strings.ToLower(m.Name), q) {
				continue
			}
			out = append(out, m)
		}
		return out
	}

	// Update filtered maps initially
	state.FilteredMaps = filterMaps()

	// Map list
	mapList := widget.NewList(
		func() int { return len(state.FilteredMaps) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, obj fyne.CanvasObject) {
			obj.(*widget.Label).SetText(state.FilteredMaps[i].Name)
		},
	)

	mapList.OnSelected = func(id widget.ListItemID) {
		if id < 0 || id >= len(state.FilteredMaps) {
			return
		}
		onMapSelected(state.FilteredMaps[id])
	}

	search.OnChanged = func(string) {
		state.FilteredMaps = filterMaps()
		mapList.Refresh()
	}

	scroll := container.NewVScroll(mapList)
	scroll.SetMinSize(fyne.NewSize(0, 240))

	mapListPanel := container.NewPadded(scroll)

	optionsBox := container.NewVBox(
		widget.NewSeparator(),
		widget.NewLabelWithStyle("Quick Options", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		hideBlizzard,
		hideNoBanks,
		autoResign,
	)

	return container.NewBorder(
		container.NewVBox(
			widget.NewLabelWithStyle("Map List", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
			widget.NewSeparator(),
			container.NewPadded(search),
		),
		container.NewPadded(optionsBox),
		nil, nil,
		mapListPanel,
	)
}
