package ui

import (
	"net/url"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	"starbank/internal/ui/utils"
	"starbank/internal/ui/widgets"

	ttwidget "github.com/dweymouth/fyne-tooltip/widget"
)

// BuildHeader creates the top section with breadcrumbs, title, and action buttons
func BuildHeader(
	state *AppState,
	window fyne.Window,
	onOpenInFS func(),
	onToolsMenu func(anchor fyne.CanvasObject),
	onSettingsMenu func(anchor fyne.CanvasObject),
) (*fyne.Container, *canvas.Text, *fyne.Container, *container.Scroll) {

	// Breadcrumbs
	initialPath := []string{
		"Home", "StarCraft II", "Accounts", "1234567",
		"1-S2-1-1234567", "Banks",
		"Lottery Defence.SC2Map", "PlayerData.SC2Bank",
	}
	crumbs := widgets.MakeBreadcrumbBar(utils.Tail(initialPath, 6))
	crumbsScroll := container.NewHScroll(crumbs)
	crumbsScroll.SetMinSize(fyne.NewSize(0, crumbs.MinSize().Height))

	// Title
	titleText := canvas.NewText("Lottery Defence", theme.Color(theme.ColorNameForeground))
	titleText.TextSize = 28
	titleText.TextStyle = fyne.TextStyle{Bold: true}

	// Info labels
	createdBy := widget.NewLabel("Created by: (Unknown)")
	lastUpdated := widget.NewLabel("Last update downloaded on 10/21/2013")

	// Bank selector hint and dropdown
	multiBankHint := canvas.NewText("Multiple bank files detected. Choose one:", theme.Color(theme.ColorNameError))
	multiBankHint.TextStyle = fyne.TextStyle{Italic: true}

	bankSelect := widget.NewSelect([]string{"sld", "default", "backup"}, func(string) {})
	bankSelect.SetSelected("sld")

	// Action buttons
	openInFSBtn := ttwidget.NewButtonWithIcon("", theme.FolderOpenIcon(), func() {
		onOpenInFS()
	})
	openInFSBtn.Importance = widget.LowImportance
	openInFSBtn.SetToolTip("Open current bank in your file manager")

	toolsBtn := ttwidget.NewButtonWithIcon("", theme.MoreHorizontalIcon(), nil)
	toolsBtn.Importance = widget.LowImportance
	toolsBtn.SetToolTip("Tools")
	toolsBtn.OnTapped = func() {
		onToolsMenu(toolsBtn)
	}

	settingsBtn := ttwidget.NewButtonWithIcon("", theme.SettingsIcon(), nil)
	settingsBtn.Importance = widget.LowImportance
	settingsBtn.SetToolTip("Settings")
	settingsBtn.OnTapped = func() {
		onSettingsMenu(settingsBtn)
	}

	// Assemble header
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
		utils.NewVGap(6),
		container.NewCenter(titleText),
		container.NewPadded(infoRow),
		chooseRow,
		widget.NewSeparator(),
	)

	return header, titleText, crumbs, crumbsScroll
}

// OpenInFileManager opens the specified path in the system file manager
func OpenInFileManager(path string) {
	if strings.TrimSpace(path) == "" {
		return
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return
	}
	fyne.CurrentApp().OpenURL(&url.URL{Scheme: "file", Path: abs})
}
