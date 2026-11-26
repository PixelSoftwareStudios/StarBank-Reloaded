package ui

import (
	"fmt"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"

	"starbank/internal/editor"
	"starbank/internal/mapinfo"
	"starbank/internal/maps"
	"starbank/internal/ui/utils"
	"starbank/internal/ui/views"
	"starbank/internal/ui/widgets"
	"starbank/pkg/bankeditorlib/bank"

	fynetooltip "github.com/dweymouth/fyne-tooltip"
)

// App is the root of the StarBank Reloaded application.
type App struct {
	fyneApp fyne.App
	window  fyne.Window
	state   *AppState
	status  *widget.Label

	// Header widgets for updating after map load.
	titleText    *canvas.Text
	crumbs       *fyne.Container
	crumbsScroll *container.Scroll

	// Called whenever filters change — refreshes the sidebar map list.
	reloadMapList func()
}

// NewApp creates and initialises a new application instance.
func NewApp() *App {
	return &App{
		fyneApp: app.New(),
		state:   NewAppState(),
	}
}

// Run starts the application event loop.
func (a *App) Run() {
	a.window = a.fyneApp.NewWindow("StarBank Reloaded v1.0 - The StarCraft II Bank Editor")
	a.window.Resize(fyne.NewSize(850, 500))

	a.status = widget.NewLabel("Ready")

	// Populate the map list from disk before building the UI so the sidebar
	// has real data when it is constructed.
	a.initSC2Path()

	content := a.buildUI()
	a.window.SetContent(fynetooltip.AddWindowToolTipLayer(content, a.window.Canvas()))
	widgets.ScrollBreadcrumbsToEnd(a.crumbsScroll)

	// Set startup status so the user knows what was (or wasn't) detected.
	a.updatePathStatus()

	a.window.ShowAndRun()
}

// initSC2Path auto-detects (or reads from settings) the SC2 root path,
// discovers all .SC2Bank files, and enriches them with Blizzard author info
// from the Battle.net map cache.
func (a *App) initSC2Path() {
	// Honour a manual override stored in settings.
	if a.state.Settings.SC2RootOverride != "" {
		if folders, err := bank.GetBankFolderFromRoot(a.state.Settings.SC2RootOverride); err == nil {
			a.state.SC2Root = a.state.Settings.SC2RootOverride
			a.state.BankFolders = folders
			a.state.AllMaps = maps.DiscoverMaps(folders)
			a.enrichMapsFromCache()
			return
		}
	}

	root, err := bank.GetSC2RootPath()
	if err != nil {
		a.state.PathLoadError = err
		return
	}

	folders, err := bank.GetBankFolderFromRoot(root)
	if err != nil {
		a.state.PathLoadError = err
		return
	}

	a.state.SC2Root = root
	a.state.BankFolders = folders
	a.state.AllMaps = maps.DiscoverMaps(folders)
	a.enrichMapsFromCache()
}

// enrichMapsFromCache scans the Battle.net map cache and populates
// IsBlizzardMap and AuthorName on each entry in state.AllMaps.
func (a *App) enrichMapsFromCache() {
	if len(a.state.AllMaps) == 0 {
		return
	}
	reg := mapinfo.ScanCache()
	fmt.Printf("[mapinfo] registry contains %d entries\n", len(reg))
	for k, v := range reg {
		fmt.Printf("[mapinfo]   bank=%q  author=%q  isBlizzard=%v\n",
			k, v.AuthorName, v.IsBlizzard())
	}

	for i := range a.state.AllMaps {
		name := strings.ToLower(a.state.AllMaps[i].Name)
		if rec, ok := reg[name]; ok {
			a.state.AllMaps[i].AuthorName = rec.AuthorName
			a.state.AllMaps[i].IsBlizzardMap = rec.IsBlizzard()
		}
		m := a.state.AllMaps[i]
		fmt.Printf("[maps] name=%q  authorID=%q  authorName=%q  isBlizzard=%v  path=%s\n",
			m.Name, m.AuthorID, m.AuthorName, m.IsBlizzardMap, m.BankPath)
	}
}

// updatePathStatus sets the status label to reflect the current load state.
func (a *App) updatePathStatus() {
	switch {
	case a.state.PathLoadError != nil:
		a.status.SetText("Could not detect SC2 path — select folder via ⚙ Settings")
	case len(a.state.AllMaps) == 0:
		a.status.SetText("No bank files found — select folder via ⚙ Settings")
	default:
		a.status.SetText(fmt.Sprintf("Found %d bank file(s) in %s",
			len(a.state.AllMaps), a.state.SC2Root))
	}
}

// buildUI constructs the complete user interface.
func (a *App) buildUI() fyne.CanvasObject {
	// Sidebar — returns a refresh function for when AllMaps changes.
	sidebar, reloadMaps := BuildSidebar(a.state, a.onMapSelected, a.onFilterChanged)
	a.reloadMapList = reloadMaps

	// Header
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

	// Editor views (still use existing builder functions for now).
	viewTabs := container.NewAppTabs(
		container.NewTabItem("Simple View", views.BuildSimpleView(a.onEdit)),
		container.NewTabItem("Table View", views.BuildTableView(a.onEdit)),
		container.NewTabItem("Raw View", views.BuildRawView(a.onEdit)),
	)

	// Bottom bar
	bottomBar := a.buildBottomBar()

	rightInner := container.NewBorder(
		header,
		bottomBar,
		nil, nil,
		viewTabs,
	)
	right := container.NewPadded(rightInner)

	split := container.NewHSplit(sidebar, right)
	split.SetOffset(0.28)

	return split
}

// buildBottomBar creates the status + save controls bar.
func (a *App) buildBottomBar() fyne.CanvasObject {
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

// --- event handlers ---

func (a *App) onMapSelected(item maps.MapItem) {
	a.state.CurrentMap = &item

	// Update the header title.
	a.titleText.Text = item.Name
	a.titleText.Refresh()

	// Build breadcrumbs from the bank path.
	breadcrumbParts := splitPathTail(item.BankPath, 6)
	widgets.SetBreadcrumbBarParts(a.crumbs, breadcrumbParts)
	widgets.ScrollBreadcrumbsToEnd(a.crumbsScroll)

	a.status.SetText("Selected: " + item.Name)

	// TODO: Load the bank file and refresh views once views are wired up.
}

func (a *App) onFilterChanged() {
	_ = a.state.Settings.Save()
	if a.reloadMapList != nil {
		a.reloadMapList()
	}
	a.status.SetText("Filters updated")
}

func (a *App) onEdit(field string) {
	a.state.Editor.MarkDirty()
	a.status.SetText("Edited: " + field)
}

func (a *App) onSave() {
	if err := a.state.Editor.Save(editor.SaveModeOverwrite, a.state.Settings.AutoResignOnSave); err != nil {
		a.status.SetText("Error saving: " + err.Error())
		return
	}
	if a.state.Settings.AutoResignOnSave {
		a.status.SetText("Saved • Re-signed")
	} else {
		a.status.SetText("Saved")
	}
}

func (a *App) onSaveOverwrite() {
	a.onSave()
}

func (a *App) onSaveAs() {
	dialog.ShowFileSave(func(uri fyne.URIWriteCloser, err error) {
		if err != nil || uri == nil {
			return
		}
		if saveErr := a.state.Editor.SaveAs(uri.URI().Path(), a.state.Settings.AutoResignOnSave); saveErr != nil {
			a.status.SetText("Error saving: " + saveErr.Error())
			return
		}
		a.status.SetText("Saved as: " + uri.URI().Path())
	}, a.window)
}

func (a *App) onSaveWithBackup() {
	if err := a.state.Editor.Save(editor.SaveModeBackup, a.state.Settings.AutoResignOnSave); err != nil {
		a.status.SetText("Error saving: " + err.Error())
		return
	}
	if a.state.Settings.AutoResignOnSave {
		a.status.SetText("Saved w/ backup • Re-signed")
	} else {
		a.status.SetText("Saved w/ backup")
	}
}

func (a *App) onCopyXML() {
	xmlContent, err := a.state.Editor.CopyXML()
	if err != nil {
		a.status.SetText("Error: " + err.Error())
		return
	}
	widgets.CopyBankXMLToClipboard(xmlContent,
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
		if a.state.CurrentMap != nil {
			path = a.state.CurrentMap.BankPath
		}
	}
	if path == "" {
		a.status.SetText("No bank file selected")
		return
	}
	OpenInFileManager(path)
}

func (a *App) onToolsMenu(anchor fyne.CanvasObject) {
	menu := fyne.NewMenu("Tools",
		fyne.NewMenuItem("Unprotect external map file…", func() {
			a.status.SetText("Unprotect external map file (not yet implemented)")
		}),
		fyne.NewMenuItem("Re-sign external bank…", func() {
			a.status.SetText("Re-sign external bank (not yet implemented)")
		}),
	)
	utils.ShowMenuUnder(a.window, anchor, menu)
}

func (a *App) onSettingsMenu(anchor fyne.CanvasObject) {
	rootLabel := a.state.SC2Root
	if rootLabel == "" {
		rootLabel = "Not detected"
	}
	menu := fyne.NewMenu("Settings",
		fyne.NewMenuItem("SC2 Root: "+rootLabel, nil),
		fyne.NewMenuItemSeparator(),
		fyne.NewMenuItem("Select SC2 folder…", func() {
			dialog.ShowFolderOpen(func(uri fyne.ListableURI, err error) {
				if err != nil || uri == nil {
					return
				}
				a.handleFolderSelected(uri.Path())
			}, a.window)
		}),
	)
	utils.ShowMenuUnder(a.window, anchor, menu)
}

// handleFolderSelected processes a user-chosen folder and reloads the map list.
func (a *App) handleFolderSelected(selectedPath string) {
	ok, sc2Root, bankFolders := bank.IsValidBankPath(selectedPath)
	if !ok || len(bankFolders) == 0 {
		a.state.PathLoadError = fmt.Errorf("not a valid SC2 folder: %s", selectedPath)
		a.state.AllMaps = nil
	} else {
		a.state.PathLoadError = nil
		a.state.SC2Root = sc2Root
		a.state.BankFolders = bankFolders
		a.state.AllMaps = maps.DiscoverMaps(bankFolders)
		a.enrichMapsFromCache()
		a.state.Settings.SC2RootOverride = selectedPath
		_ = a.state.Settings.Save()
	}

	if a.reloadMapList != nil {
		a.reloadMapList()
	}
	a.updatePathStatus()
}

// splitPathTail returns the last n path components of p for breadcrumb display.
func splitPathTail(p string, n int) []string {
	if p == "" {
		return nil
	}
	// Split on both / and \ for cross-platform paths.
	var parts []string
	start := 0
	for i, c := range p {
		if c == '/' || c == '\\' {
			if i > start {
				parts = append(parts, p[start:i])
			}
			start = i + 1
		}
	}
	if start < len(p) {
		parts = append(parts, p[start:])
	}
	return utils.Tail(parts, n)
}
