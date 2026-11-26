package views

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

// BuildRawView creates a raw XML editor view
func BuildRawView(onEdit func(field string)) fyne.CanvasObject {
	editor := widget.NewMultiLineEntry()
	editor.Wrapping = fyne.TextWrapOff

	// TODO: Load from actual bank editor
	// For now, use mock XML
	mockXML := `<?xml version="1.0" encoding="utf-8"?>
<Bank>
	<Section name="xp">
		<Key name="xp" type="int">10000000943</Key>
	</Section>
</Bank>`
	editor.SetText(mockXML)

	editor.OnChanged = func(s string) {
		// TODO: Update bank XML in editor
		onEdit("Raw XML")
	}

	return container.NewPadded(editor)
}
