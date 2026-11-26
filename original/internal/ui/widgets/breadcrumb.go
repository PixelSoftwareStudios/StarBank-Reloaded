package widgets

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
)

// MakeBreadcrumbBar creates a breadcrumb navigation bar from path parts
func MakeBreadcrumbBar(parts []string) *fyne.Container {
	return container.NewHBox(buildBreadcrumbObjects(parts)...)
}

// SetBreadcrumbBarParts updates the breadcrumb bar with new path parts
func SetBreadcrumbBarParts(bar *fyne.Container, parts []string) {
	bar.Objects = buildBreadcrumbObjects(parts)
	bar.Refresh()
}

// ScrollBreadcrumbsToEnd scrolls the breadcrumb container to show the end
func ScrollBreadcrumbsToEnd(sc *container.Scroll) {
	sc.ScrollToOffset(fyne.NewPos(1000000, 0))
}

// buildBreadcrumbObjects converts path parts into canvas objects for display
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
