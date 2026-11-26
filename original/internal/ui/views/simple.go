package views

import (
	"starbank/internal/ui/utils"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// BuildSimpleView creates a form-based view for editing common bank fields
func BuildSimpleView(onEdit func(field string)) fyne.CanvasObject {
	player := widget.NewEntry()
	player.SetText("Dante") // TODO: Load from actual bank data

	gold := widget.NewEntry()
	gold.SetText("1,000,000") // TODO: Load from actual bank data
	gold.OnChanged = func(string) {
		onEdit("Gold")
	}

	level := widget.NewEntry()
	level.SetText("50") // TODO: Load from actual bank data
	level.OnChanged = func(string) {
		onEdit("Level")
	}

	form := widget.NewForm(
		widget.NewFormItem("Player", player),
		widget.NewFormItem("Gold", gold),
		widget.NewFormItem("Level", level),
	)

	return container.NewPadded(
		container.NewVBox(
			utils.NewVGap(8),
			form,
		),
	)
}
