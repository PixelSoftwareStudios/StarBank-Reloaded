# PLAN.md

## StarBank Reloaded -- Development Plan

StarBank Reloaded is a modern, cross-platform replacement for the
original **StarBank** editor used for modifying **StarCraft II
`.SC2Bank` files**.

The application is written in **Go**, uses **Fyne** for the GUI, and
separates logic into a reusable library (`bankeditor-lib`).

The goal is to create a **clean, fast, lightweight, cross-platform SC2
bank editor** with improved UX and maintainable architecture.

------------------------------------------------------------------------

# Core Architecture

## Packages

cmd/starbank\
main.go\
GUI application

pkg/bankeditor-lib\
model.go\
parser.go\
signature.go\
writer.go\
paths.go\
detection.go\
sorting.go

The `bankeditor-lib` package must contain **all non-GUI logic** so it
can be reused by:

-   CLI tools
-   other GUIs
-   automated bank editing scripts

------------------------------------------------------------------------

# Bank File Model

Replicate the structure used in the original StarBank codebase.

### Types

Bank\
├─ BankInfo\
├─ Sections \[\]\
│ └─ Keys \[\]\
├─ Signature\
└─ Version

### Structs

Bank\
Section\
Key\
BankInfo

------------------------------------------------------------------------

# Bank Parsing

SC2 bank files are **plain XML files**.

Example structure:

`<Bank>`{=html}
```{=html}
<Section name="xp">
```
`<Key name="xp" type="int">`{=html}123`</Key>`{=html}
```{=html}
</Section>
```
`</Bank>`{=html}

### Parser responsibilities

`parser.go` must:

-   Parse XML → Bank struct
-   Preserve ordering
-   Preserve types
-   Preserve section structure

Functions:

ParseBankFile(path string) (*Bank, error)\
ParseBankXML(xml string) (*Bank, error)

------------------------------------------------------------------------

# Bank Serialization

After editing the bank must be written back to disk.

Requirements:

-   Preserve formatting where possible
-   Recalculate signature
-   Support atomic writes
-   Optionally create backups

Functions:

SerializeBank(bank *Bank) (\[\]byte, error)\
WriteBank(path string, bank *Bank) error\
WriteBankWithBackup(path string, bank \*Bank) error

------------------------------------------------------------------------

# Bank Signature (Re-Signing)

StarCraft II validates bank files using a **SHA1 checksum signature**.

Algorithm:

1.  Concatenate ordered section + key data
2.  Compute SHA1 hash
3.  Write uppercase hex string to `<Signature>`

Implementation:

signature.go

Functions:

GenerateSignature(bank *Bank) string\
UpdateSignature(bank *Bank) error

Sorting rules:

-   Sections ordered by **ASCII**
-   Keys ordered by **ASCII**

Equivalent to C#:

OrderBy(o =\> o.Name, StringComparer.Ordinal)

------------------------------------------------------------------------

# Sorting Helpers

Functions:

OrderSectionsByNameASCII(\[\]*Section) \[\]*Section\
OrderKeysByNameASCII(\[\]*Key) \[\]*Key

Requirements:

-   Stable sort
-   Returns **new slice**
-   Does not mutate original data

------------------------------------------------------------------------

# Atomic File Writes

Bank files should never be partially written.

Write process:

temp file → write → fsync → replace original

Additional behavior:

-   Preserve original file permissions when possible
-   Optional `.bak_timestamp` backup creation

Functions:

WriteFileAtomic(path string, data \[\]byte) error\
WriteFileWithBackup(path string, data \[\]byte) error

------------------------------------------------------------------------

# Path Detection

Functions:

GetSC2RootPath()\
GetBankFolderFromRoot()\
IsValidBankPath()

Windows path example:

Documents\\StarCraft II

macOS path example:

\~/Library/Application Support/Blizzard/StarCraft II

Linux: stub returning not implemented.

------------------------------------------------------------------------

# GUI Features

Left Panel\
- Map List\
- Search\
- Quick Options

Right Panel\
- Breadcrumbs\
- Map Title\
- Bank Selection\
- Editor Views\
- Save Controls

------------------------------------------------------------------------

# Quick Options

Hide Blizzard maps\
Hide maps without bank files\
Auto re-sign banks on save

------------------------------------------------------------------------

# Editor Views

Simple View -- friendly editing for common values

Table View -- full structured editing

Raw View -- XML editor that re-signs on save

------------------------------------------------------------------------

# Save Controls

Save (overwrite)\
Save As\
Save w/ Backup\
Copy bank XML to clipboard

------------------------------------------------------------------------

# Tools Menu

Unprotect external map file\
Re-sign external bank

------------------------------------------------------------------------

# Performance Goals

Minimal memory usage\
Avoid unnecessary allocations\
Avoid repeated XML parsing

------------------------------------------------------------------------

# Cross Platform Support

Windows\
macOS\
Linux (manual path selection initially)

------------------------------------------------------------------------

# License

MIT License
