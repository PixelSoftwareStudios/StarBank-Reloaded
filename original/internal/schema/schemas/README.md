# Bank Schemas

YAML files in this directory define the Simple View for specific maps.
Each file maps cryptic SC2Bank section/key names to human-readable labels
and editor widget types.

## Format

```yaml
# map: must match the .SC2Bank filename without extension, case-insensitive.
map: "BankName"
groups:
  - label: "Group Label"
    fields:
      - label: "Human-readable name"
        section: "bank_section_name"
        key: "bank_key_name"
        type: number     # number | checkbox | string
```

## Field Types

| type       | Widget     | Storage         |
|------------|------------|-----------------|
| `number`   | Text entry | Raw number; commas stripped on save |
| `checkbox` | Checkbox   | `"1"` (checked) or `"0"` (unchecked) |
| `string`   | Text entry | Raw string      |

## Adding a schema

1. Open the bank in Table View to see the real section and key names.
2. Create a new `.yaml` file in this directory.
3. Restart the app — schemas are embedded at compile time.
