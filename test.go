package main

import (
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

func main() {
	a := app.New()
	w := a.NewWindow("StarBank Reloaded - The StarCraft II Bank File Editor")
	w.Resize(fyne.NewSize(800, 500))

	hello := widget.NewLabel("Hello Fyne!")
	w.SetContent(container.NewVBox(
		hello,
		widget.NewButton("Hi!", func() {
			hello.SetText(fmt.Sprintf("Size: %.2f, %.2f", w.Content().Size().Width, w.Content().Size().Height))
		}),
	))

	w.ShowAndRun()
}
