package main

import (
	"fmt"
	"image/color"
	"net/url"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	fynetooltip "github.com/dweymouth/fyne-tooltip"
	ttwidget "github.com/dweymouth/fyne-tooltip/widget"
)

type UIState struct {
	CurrentPath string
	Dirty       bool

	HideBlizzardMaps     bool
	HideMapsWithoutBanks bool
	AutoResignOnSave     bool

	CurrentBankXML string
}

type MapItem struct {
	Name          string
	HasBankFiles  bool
	IsBlizzardMap bool
}

func main() {
	a := app.New()
	w := a.NewWindow("StarBank Reloaded")
	w.Resize(fyne.NewSize(850, 500))

	state := &UIState{
		CurrentPath:          "/Users/dante/Documents/StarCraft II/Accounts/1234567/1-S2-1-1234567/Banks/Lottery Defence.SC2Map/PlayerData.SC2Bank",
		Dirty:                true,
		HideBlizzardMaps:     true,
		HideMapsWithoutBanks: true,
		AutoResignOnSave:     true,
		CurrentBankXML: `<?xml version="1.0" encoding="utf-8"?>
<Bank>
	<Section name="xp">
		<Key name="xp" type="int">10000000943</Key>
	</Section>
</Bank>`,
	}

	status := widget.NewLabel("Ready")

	search := widget.NewEntry()
	search.SetPlaceHolder("Search maps…")

	allMaps := []MapItem{
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

	filteredMaps := func() []MapItem {
		q := strings.ToLower(strings.TrimSpace(search.Text))
		out := []MapItem{}
		for _, m := range allMaps {
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

	curMaps := filteredMaps()

	mapList := widget.NewList(
		func() int { return len(curMaps) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, obj fyne.CanvasObject) {
			obj.(*widget.Label).SetText(curMaps[i].Name)
		},
	)

	crumbs := makeBreadcrumbBar(tail([]string{
		"Home", "StarCraft II", "Accounts", "1234567",
		"1-S2-1-1234567", "Banks",
		"Lottery Defence.SC2Map", "PlayerData.SC2Bank",
	}, 6))

	crumbsScroll := container.NewHScroll(crumbs)
	crumbsScroll.SetMinSize(fyne.NewSize(0, crumbs.MinSize().Height))

	titleText := canvas.NewText("Lottery Defence", theme.Color(theme.ColorNameForeground))
	titleText.TextSize = 28
	titleText.TextStyle = fyne.TextStyle{Bold: true}

	createdBy := widget.NewLabel("Created by: (Unknown)")
	lastUpdated := widget.NewLabel("Last update downloaded on 10/21/2013")

	multiBankHint := canvas.NewText("Multiple bank files detected. Choose one:", theme.Color(theme.ColorNameError))
	multiBankHint.TextStyle = fyne.TextStyle{Italic: true}

	bankSelect := widget.NewSelect([]string{"sld", "default", "backup"}, func(string) {})
	bankSelect.SetSelected("sld")

	openInFSBtn := ttwidget.NewButtonWithIcon("", theme.FolderOpenIcon(), func() {
		openInFileManager(state.CurrentPath)
	})
	openInFSBtn.Importance = widget.LowImportance
	openInFSBtn.SetToolTip("Open current bank in your file manager")

	toolsBtn := ttwidget.NewButtonWithIcon("", theme.MoreHorizontalIcon(), nil)
	toolsBtn.Importance = widget.LowImportance
	toolsBtn.SetToolTip("Tools")

	settingsBtn := ttwidget.NewButtonWithIcon("", theme.SettingsIcon(), nil)
	settingsBtn.Importance = widget.LowImportance
	settingsBtn.SetToolTip("Settings")

	toolsBtn.OnTapped = func() {
		menu := fyne.NewMenu("Tools",
			fyne.NewMenuItem("Unprotect external map file…", func() {
				status.SetText("Unprotect external map file (mock)")
			}),
			fyne.NewMenuItem("Re-sign external bank…", func() {
				status.SetText("Re-sign external bank (mock)")
			}),
		)
		showMenuUnder(w, toolsBtn, menu)
	}

	settingsBtn.OnTapped = func() {
		menu := fyne.NewMenu("Settings",
			fyne.NewMenuItem("More…", func() {
				status.SetText("Settings page (mock)")
			}),
		)
		showMenuUnder(w, settingsBtn, menu)
	}

	mapList.OnSelected = func(id widget.ListItemID) {
		if id < 0 || id >= len(curMaps) {
			return
		}
		name := curMaps[id].Name
		titleText.Text = name
		titleText.Refresh()

		state.CurrentPath = fmt.Sprintf("/.../Banks/%s.SC2Map/PlayerData.SC2Bank", name)
		state.Dirty = false

		setBreadcrumbBarParts(crumbs, tail([]string{
			"Home", "StarCraft II", "Accounts", "1234567",
			"1-S2-1-1234567", "Banks",
			name + ".SC2Map", "PlayerData.SC2Bank",
		}, 6))

		scrollBreadcrumbsToEnd(crumbsScroll)
		status.SetText("Selected map: " + name)
	}

	hideBlizzard := widget.NewCheck("Hide Blizzard maps", func(v bool) {
		state.HideBlizzardMaps = v
		curMaps = filteredMaps()
		mapList.Refresh()
	})
	hideBlizzard.SetChecked(state.HideBlizzardMaps)

	hideNoBanks := widget.NewCheck("Hide maps without bank files", func(v bool) {
		state.HideMapsWithoutBanks = v
		curMaps = filteredMaps()
		mapList.Refresh()
	})
	hideNoBanks.SetChecked(state.HideMapsWithoutBanks)

	autoResign := widget.NewCheck("Auto re-sign banks on save", func(v bool) {
		state.AutoResignOnSave = v
	})
	autoResign.SetChecked(state.AutoResignOnSave)

	search.OnChanged = func(string) {
		curMaps = filteredMaps()
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

	left := container.NewBorder(
		container.NewVBox(
			widget.NewLabelWithStyle("Map List", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
			widget.NewSeparator(),
			container.NewPadded(search),
		),
		container.NewPadded(optionsBox),
		nil, nil,
		mapListPanel,
	)

	views := container.NewAppTabs(
		container.NewTabItem("Simple View", buildSimpleView(state, status)),
		container.NewTabItem("Table View", buildTableView(state, status)),
		container.NewTabItem("Raw View", buildRawView(state, status)),
	)

	crumbRow := container.NewBorder(
		nil, nil,
		nil,
		container.NewHBox(openInFSBtn, toolsBtn, settingsBtn),
		crumbsScroll,
	)

	infoRow := container.NewBorder(nil, nil, createdBy, lastUpdated, nil)
	chooseRow := container.NewBorder(nil, nil, multiBankHint, bankSelect, nil)

	header := container.NewVBox(
		crumbRow,
		vgap(6),
		container.NewCenter(titleText),
		container.NewPadded(infoRow),
		chooseRow,
		widget.NewSeparator(),
	)

	saveBtn, saveMenuBtn := makeSaveSplitButtons(w, state, status)

	resetBtn := ttwidget.NewButton("Reset", func() {
		state.Dirty = false
		status.SetText("Reverted")
	})
	resetBtn.Importance = widget.LowImportance
	resetBtn.SetToolTip("Reset values to match the original file")

	saveGroup := container.NewHBox(resetBtn, hgap(8), saveBtn, saveMenuBtn)

	bottomBar := container.NewVBox(
		container.NewBorder(
			nil, nil,
			status,
			saveGroup,
			nil,
		),
		vgap(2),
	)

	rightInner := container.NewBorder(
		header,
		bottomBar,
		nil, nil,
		views,
	)

	right := container.NewPadded(rightInner)

	split := container.NewHSplit(left, right)
	split.SetOffset(0.28)

	w.SetContent(fynetooltip.AddWindowToolTipLayer(split, w.Canvas()))

	scrollBreadcrumbsToEnd(crumbsScroll)

	w.ShowAndRun()
}

func scrollBreadcrumbsToEnd(sc *container.Scroll) {
	sc.ScrollToOffset(fyne.NewPos(1000000, 0))
}

func showMenuUnder(w fyne.Window, anchor fyne.CanvasObject, menu *fyne.Menu) {
	pop := widget.NewPopUpMenu(menu, w.Canvas())
	pos := fyne.CurrentApp().Driver().AbsolutePositionForObject(anchor)
	pop.ShowAtPosition(fyne.NewPos(pos.X, pos.Y+anchor.Size().Height))
}

func makeSaveSplitButtons(
	w fyne.Window,
	state *UIState,
	status *widget.Label,
) (fyne.CanvasObject, fyne.CanvasObject) {

	saveBtn := ttwidget.NewButtonWithIcon("Save", theme.DocumentSaveIcon(), func() {
		if state.AutoResignOnSave {
			status.SetText("Saved • Re-signed (mock)")
		} else {
			status.SetText("Saved (mock)")
		}
		state.Dirty = false
	})
	saveBtn.Importance = widget.HighImportance
	saveBtn.SetToolTip("Save changes")

	menuBtn := ttwidget.NewButtonWithIcon("", theme.MenuDropDownIcon(), nil)
	menuBtn.Importance = widget.LowImportance
	menuBtn.SetToolTip("Save options")

	menuBtn.OnTapped = func() {
		menu := fyne.NewMenu("",
			fyne.NewMenuItem("Save (Overwrite)", func() {
				if state.AutoResignOnSave {
					status.SetText("Saved (overwrite) • Re-signed (mock)")
				} else {
					status.SetText("Saved (overwrite) (mock)")
				}
				state.Dirty = false
			}),
			fyne.NewMenuItem("Save As…", func() {
				status.SetText("Save As… (mock)")
			}),
			fyne.NewMenuItem("Save w/ Backup", func() {
				if state.AutoResignOnSave {
					status.SetText("Saved w/ backup • Re-signed (mock)")
				} else {
					status.SetText("Saved w/ backup (mock)")
				}
				state.Dirty = false
			}),
			fyne.NewMenuItemSeparator(),
			fyne.NewMenuItem("Copy bank XML to clipboard", func() {
				copyBankXMLToClipboard(w, state, status)
			}),
		)
		showMenuUnder(w, menuBtn, menu)
	}

	return saveBtn, menuBtn
}

func copyBankXMLToClipboard(w fyne.Window, state *UIState, status *widget.Label) {
	xml := strings.TrimSpace(state.CurrentBankXML)
	if xml == "" {
		status.SetText("No bank XML loaded")
		return
	}
	w.Clipboard().SetContent(xml)
	status.SetText("Copied bank XML to clipboard")
}

func buildSimpleView(state *UIState, status *widget.Label) fyne.CanvasObject {
	player := widget.NewEntry()
	player.SetText("Dante")

	gold := widget.NewEntry()
	gold.SetText("1,000,000")
	gold.OnChanged = func(string) {
		state.Dirty = true
		status.SetText("Edited: Gold")
	}

	level := widget.NewEntry()
	level.SetText("50")

	form := widget.NewForm(
		widget.NewFormItem("Player", player),
		widget.NewFormItem("Gold", gold),
		widget.NewFormItem("Level", level),
	)

	return container.NewPadded(
		container.NewVBox(
			vgap(8),
			form,
		),
	)
}

func buildTableView(state *UIState, status *widget.Label) fyne.CanvasObject {
	sections := []string{"1", "cl", "tp", "tr", "ts", "xp"}
	selected := "xp"

	rows := map[string][][3]string{
		"xp": {{"xp", "int", "10000000943"}},
	}

	secList := widget.NewList(
		func() int { return len(sections) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, obj fyne.CanvasObject) {
			obj.(*widget.Label).SetText(sections[i])
		},
	)

	valueList := widget.NewList(
		func() int { return len(rows[selected]) },
		func() fyne.CanvasObject {
			return container.NewGridWithColumns(3,
				widget.NewLabel(""),
				widget.NewLabel(""),
				widget.NewEntry(),
			)
		},
		func(i widget.ListItemID, obj fyne.CanvasObject) {
			grid := obj.(*fyne.Container)
			grid.Objects[0].(*widget.Label).SetText(rows[selected][i][0])
			grid.Objects[1].(*widget.Label).SetText(rows[selected][i][1])
			grid.Objects[2].(*widget.Entry).SetText(rows[selected][i][2])
		},
	)

	secList.OnSelected = func(id widget.ListItemID) {
		selected = sections[id]
		valueList.Refresh()
	}

	leftPane := container.NewBorder(
		widget.NewLabelWithStyle("Sections", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		nil, nil, nil,
		secList,
	)

	headerRow := container.NewGridWithColumns(3,
		widget.NewLabelWithStyle("Name", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		widget.NewLabelWithStyle("Type", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		widget.NewLabelWithStyle("Value", fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
	)

	rightPane := container.NewBorder(
		headerRow,
		nil, nil, nil,
		valueList,
	)

	split := container.NewHSplit(leftPane, rightPane)
	split.SetOffset(0.22)

	return container.NewPadded(split)
}

func buildRawView(state *UIState, status *widget.Label) fyne.CanvasObject {
	editor := widget.NewMultiLineEntry()
	editor.Wrapping = fyne.TextWrapOff
	editor.SetText(state.CurrentBankXML)

	editor.OnChanged = func(s string) {
		state.CurrentBankXML = s
		state.Dirty = true
		status.SetText("Edited: Raw XML")
	}

	return container.NewPadded(editor)
}

func makeBreadcrumbBar(parts []string) *fyne.Container {
	return container.NewHBox(buildBreadcrumbObjects(parts)...)
}

func setBreadcrumbBarParts(bar *fyne.Container, parts []string) {
	bar.Objects = buildBreadcrumbObjects(parts)
	bar.Refresh()
}

func buildBreadcrumbObjects(parts []string) []fyne.CanvasObject {
	objs := []fyne.CanvasObject{}
	disabled := theme.Color(theme.ColorNameDisabled)
	active := theme.Color(theme.ColorNameForeground)

	for i, p := range parts {
		if i > 0 {
			objs = append(objs, canvas.NewText("  ›  ", disabled))
		}
		t := canvas.NewText(p, disabled)
		if i == len(parts)-1 {
			t.Color = active
			t.TextStyle = fyne.TextStyle{Bold: true}
		}
		objs = append(objs, t)
	}
	return objs
}

func tail(parts []string, n int) []string {
	if len(parts) <= n {
		return parts
	}
	return append([]string{"…"}, parts[len(parts)-n:]...)
}

func openInFileManager(path string) {
	if strings.TrimSpace(path) == "" {
		return
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return
	}
	fyne.CurrentApp().OpenURL(&url.URL{Scheme: "file", Path: abs})
}

func hgap(px float32) fyne.CanvasObject {
	r := canvas.NewRectangle(color.Transparent)
	r.SetMinSize(fyne.NewSize(px, 1))
	return r
}

func vgap(px float32) fyne.CanvasObject {
	r := canvas.NewRectangle(color.Transparent)
	r.SetMinSize(fyne.NewSize(1, px))
	return r
}
