package ui

import (
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	"starbank/internal/editor"
	"starbank/internal/ui/utils"
	"starbank/internal/ui/views"
	"starbank/internal/ui/widgets"

	fynetooltip "github.com/dweymouth/fyne-tooltip"
)

// App represents the StarBank Reloaded application
type App struct {
	fyneApp fyne.App
	window  fyne.Window
	state   *AppState
	status  *widget.Label

	// UI components
	titleText    *canvas.Text
	crumbs       *fyne.Container
	crumbsScroll *container.Scroll
}

// NewApp creates and initializes a new application
func NewApp() *App {
	return &App{
		fyneApp: app.New(),
		state:   NewAppState(),
	}
}

// Run starts the application
func (a *App) Run() {
	a.window = a.fyneApp.NewWindow("StarBank Reloaded")
	a.window.Resize(fyne.NewSize(850, 500))

	a.status = widget.NewLabel("Ready")

	// Build UI
	content := a.buildUI()

	a.window.SetContent(fynetooltip.AddWindowToolTipLayer(content, a.window.Canvas()))
	widgets.ScrollBreadcrumbsToEnd(a.crumbsScroll)

	a.window.ShowAndRun()
}

// buildUI constructs the complete user interface
func (a *App) buildUI() fyne.CanvasObject {
	// Build sidebar
	sidebar := BuildSidebar(a.state, a.onMapSelected, a.onFilterChanged)

	// Build header
	header, titleText, crumbs, crumbsScroll := BuildHeader(
		a.state,
		a.window,
		a.onOpenInFileManager,
		a.onToolsMenu,
		a.onSettingsMenu,
	)
	a.titleText = titleText
	a.crumbs = crumbs
	a.crumbsScroll = crumbsScroll

	// Build views
	viewTabs := container.NewAppTabs(
		container.NewTabItem("Simple View", views.BuildSimpleView(a.onEdit)),
		container.NewTabItem("Table View", views.BuildTableView(a.onEdit)),
		container.NewTabItem("Raw View", views.BuildRawView(a.onEdit)),
	)

	// Build bottom bar with save buttons
	bottomBar := a.buildBottomBar()

	// Assemble right panel
	rightInner := container.NewBorder(
		header,
		bottomBar,
		nil, nil,
		viewTabs,
	)
	right := container.NewPadded(rightInner)

	// Create split view
	split := container.NewHSplit(sidebar, right)
	split.SetOffset(0.28)

	return split
}

// buildBottomBar creates the bottom status and save button bar
func (a *App) buildBottomBar() fyne.CanvasObject {
	// Save bar with callbacks
	saveCallbacks := widgets.SaveBarCallbacks{
		OnSave:           a.onSave,
		OnSaveOverwrite:  a.onSaveOverwrite,
		OnSaveAs:         a.onSaveAs,
		OnSaveWithBackup: a.onSaveWithBackup,
		OnCopyXML:        a.onCopyXML,
		OnReset:          a.onReset,
	}

	saveBar := widgets.MakeSaveBar(a.window, saveCallbacks)

	return container.NewVBox(
		container.NewBorder(
			nil, nil,
			a.status,
			saveBar,
			nil,
		),
		utils.NewVGap(2),
	)
}

// Event handlers

func (a *App) onMapSelected(item MapItem) {
	a.state.CurrentMapName = item.Name
	a.titleText.Text = item.Name
	a.titleText.Refresh()

	// Update breadcrumbs
	newPath := []string{
		"Home", "StarCraft II", "Accounts", "1234567",
		"1-S2-1-1234567", "Banks",
		item.Name + ".SC2Map", "PlayerData.SC2Bank",
	}
	widgets.SetBreadcrumbBarParts(a.crumbs, utils.Tail(newPath, 6))
	widgets.ScrollBreadcrumbsToEnd(a.crumbsScroll)

	a.status.SetText("Selected map: " + item.Name)

	// TODO: Load actual bank file
}

func (a *App) onFilterChanged() {
	// Filters are already applied in sidebar
	a.status.SetText("Filters updated")
}

func (a *App) onEdit(field string) {
	// Mark editor as dirty
	a.state.Editor.MarkDirty()
	a.status.SetText("Edited: " + field)
}

func (a *App) onSave() {
	if err := a.state.Editor.Save(editor.SaveModeOverwrite, a.state.AutoResignOnSave); err != nil {
		a.status.SetText("Error saving: " + err.Error())
		return
	}

	if a.state.AutoResignOnSave {
		a.status.SetText("Saved • Re-signed")
	} else {
		a.status.SetText("Saved")
	}
}

func (a *App) onSaveOverwrite() {
	if err := a.state.Editor.Save(editor.SaveModeOverwrite, a.state.AutoResignOnSave); err != nil {
		a.status.SetText("Error saving: " + err.Error())
		return
	}

	if a.state.AutoResignOnSave {
		a.status.SetText("Saved (overwrite) • Re-signed")
	} else {
		a.status.SetText("Saved (overwrite)")
	}
}

func (a *App) onSaveAs() {
	// TODO: Show file dialog for Save As
	a.status.SetText("Save As… (not implemented)")
}

func (a *App) onSaveWithBackup() {
	if err := a.state.Editor.Save(editor.SaveModeBackup, a.state.AutoResignOnSave); err != nil {
		a.status.SetText("Error saving: " + err.Error())
		return
	}

	if a.state.AutoResignOnSave {
		a.status.SetText("Saved w/ backup • Re-signed")
	} else {
		a.status.SetText("Saved w/ backup")
	}
}

func (a *App) onCopyXML() {
	xml, err := a.state.Editor.CopyXML()
	if err != nil {
		a.status.SetText("Error: " + err.Error())
		return
	}

	widgets.CopyBankXMLToClipboard(xml,
		func(msg string) { a.status.SetText(msg) },
		func(msg string) { a.status.SetText(msg) },
	)
}

func (a *App) onReset() {
	if err := a.state.Editor.Reset(); err != nil {
		a.status.SetText("Error resetting: " + err.Error())
		return
	}
	a.status.SetText("Reverted")
}

func (a *App) onOpenInFileManager() {
	path := a.state.Editor.GetFilePath()
	if path == "" {
		path = fmt.Sprintf("/Users/dante/Documents/StarCraft II/Accounts/1234567/1-S2-1-1234567/Banks/%s.SC2Map/PlayerData.SC2Bank", a.state.CurrentMapName)
	}
	OpenInFileManager(path)
}

func (a *App) onToolsMenu(anchor fyne.CanvasObject) {
	menu := fyne.NewMenu("Tools",
		fyne.NewMenuItem("Unprotect external map file…", func() {
			a.status.SetText("Unprotect external map file (not implemented)")
		}),
		fyne.NewMenuItem("Re-sign external bank…", func() {
			a.status.SetText("Re-sign external bank (not implemented)")
		}),
	)
	utils.ShowMenuUnder(a.window, anchor, menu)
}

func (a *App) onSettingsMenu(anchor fyne.CanvasObject) {
	menu := fyne.NewMenu("Settings",
		fyne.NewMenuItem("More…", func() {
			a.status.SetText("Settings page (not implemented)")
		}),
	)
	utils.ShowMenuUnder(a.window, anchor, menu)
}
