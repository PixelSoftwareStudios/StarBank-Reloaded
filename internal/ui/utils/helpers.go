package utils

import (
	"image/color"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"
)

// hgap creates a horizontal spacer with the specified pixel width
func NewHGap(px float32) fyne.CanvasObject {
	r := canvas.NewRectangle(color.Transparent)
	r.SetMinSize(fyne.NewSize(px, 1))
	return r
}

// vgap creates a vertical spacer with the specified pixel height
func NewVGap(px float32) fyne.CanvasObject {
	r := canvas.NewRectangle(color.Transparent)
	r.SetMinSize(fyne.NewSize(1, px))
	return r
}

// showMenuUnder displays a popup menu below the specified anchor widget
func ShowMenuUnder(w fyne.Window, anchor fyne.CanvasObject, menu *fyne.Menu) {
	pop := widget.NewPopUpMenu(menu, w.Canvas())
	pos := fyne.CurrentApp().Driver().AbsolutePositionForObject(anchor)
	pop.ShowAtPosition(fyne.NewPos(pos.X, pos.Y+anchor.Size().Height))
}

// tail returns the last n elements of a slice, prepended with "…" if truncated
func Tail(parts []string, n int) []string {
	if len(parts) <= n {
		return parts
	}
	return append([]string{"…"}, parts[len(parts)-n:]...)
}
