package views

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// BuildTableView creates a table-based view for editing bank sections and keys
func BuildTableView(onEdit func(field string)) fyne.CanvasObject {
	// TODO: Load from actual bank data
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
