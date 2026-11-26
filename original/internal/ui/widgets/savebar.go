package widgets

import (
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	ttwidget "github.com/dweymouth/fyne-tooltip/widget"
)

// SaveBarCallbacks contains callbacks for save bar actions
type SaveBarCallbacks struct {
	OnSave           func()
	OnSaveOverwrite  func()
	OnSaveAs         func()
	OnSaveWithBackup func()
	OnCopyXML        func()
	OnReset          func()
}

// MakeSaveBar creates a save button group with reset button
func MakeSaveBar(window fyne.Window, callbacks SaveBarCallbacks) fyne.CanvasObject {
	resetBtn := ttwidget.NewButton("Reset", callbacks.OnReset)
	resetBtn.Importance = widget.LowImportance
	resetBtn.SetToolTip("Reset values to match the original file")

	saveBtn, saveMenuBtn := makeSaveSplitButtons(window, callbacks)

	return container.New(
		&saveBarLayout{},
		resetBtn,
		saveBtn,
		saveMenuBtn,
	)
}

// makeSaveSplitButtons creates the save button and dropdown menu button
func makeSaveSplitButtons(window fyne.Window, callbacks SaveBarCallbacks) (fyne.CanvasObject, fyne.CanvasObject) {
	saveBtn := ttwidget.NewButtonWithIcon("Save", theme.DocumentSaveIcon(), callbacks.OnSave)
	saveBtn.Importance = widget.HighImportance
	saveBtn.SetToolTip("Save changes")

	menuBtn := ttwidget.NewButtonWithIcon("", theme.MenuDropDownIcon(), nil)
	menuBtn.Importance = widget.LowImportance

	// Wire up menu button to show save options
	menuBtn.OnTapped = func() {
		ShowSaveMenu(window, menuBtn, callbacks)
	}

	return saveBtn, menuBtn
}

// ShowSaveMenu displays the save options menu under the button
func ShowSaveMenu(window fyne.Window, anchor fyne.CanvasObject, callbacks SaveBarCallbacks) {
	menu := fyne.NewMenu("",
		fyne.NewMenuItem("Save (Overwrite)", callbacks.OnSaveOverwrite),
		fyne.NewMenuItem("Save As…", callbacks.OnSaveAs),
		fyne.NewMenuItem("Save w/ Backup", callbacks.OnSaveWithBackup),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("Copy bank XML to clipboard", callbacks.OnCopyXML),
	)

	pop := widget.NewPopUpMenu(menu, window.Canvas())
	pos := fyne.CurrentApp().Driver().AbsolutePositionForObject(anchor)
	pop.ShowAtPosition(fyne.NewPos(pos.X, pos.Y+anchor.Size().Height))
}

// CopyBankXMLToClipboard copies the current bank XML to clipboard
func CopyBankXMLToClipboard(xml string, onSuccess, onError func(string)) {
	xml = strings.TrimSpace(xml)
	if xml == "" {
		onError("No bank XML loaded")
		return
	}
	fyne.CurrentApp().Clipboard().SetContent(xml)
	onSuccess("Copied bank XML to clipboard")
}

// saveBarLayout arranges save bar components with proper spacing
type saveBarLayout struct{}

func (l *saveBarLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	if len(objects) == 0 {
		return fyne.NewSize(0, 0)
	}

	resetBtn := objects[0]
	saveBtn := objects[1]
	menuBtn := objects[2]

	height := fyne.Max(resetBtn.MinSize().Height, fyne.Max(saveBtn.MinSize().Height, menuBtn.MinSize().Height))
	width := resetBtn.MinSize().Width + 8 + saveBtn.MinSize().Width + menuBtn.MinSize().Width

	return fyne.NewSize(width, height)
}

func (l *saveBarLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	if len(objects) < 3 {
		return
	}

	resetBtn := objects[0]
	saveBtn := objects[1]
	menuBtn := objects[2]

	resetSize := resetBtn.MinSize()
	saveSize := saveBtn.MinSize()
	menuSize := menuBtn.MinSize()

	// Position reset button on the left
	resetBtn.Resize(resetSize)
	resetBtn.Move(fyne.NewPos(0, 0))

	// Position save button with gap
	saveBtn.Resize(saveSize)
	saveBtn.Move(fyne.NewPos(resetSize.Width+8, 0))

	// Position menu button immediately after save button
	menuBtn.Resize(menuSize)
	menuBtn.Move(fyne.NewPos(resetSize.Width+8+saveSize.Width, 0))
}
