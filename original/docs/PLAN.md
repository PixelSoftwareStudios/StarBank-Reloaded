# PLAN.md

## StarBank Reloaded -- Development Plan

StarBank Reloaded is a modern, cross-platform replacement for the
original [**StarBank**](https://github.com/BlueRaja/Starbank) editor used for modifying **StarCraft II
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

Number input behavior

commas are valid in numeric inputs

commas should be stripped before writing/signing

display can remain formatted while stored value is normalized

Save behavior

auto re-sign on save

save options:

overwrite

save as

save with backup

copy full bank XML to clipboard

Path handling

auto-detect SC2 root on Windows/macOS

Linux stub returns not implemented

manual folder selection must always be supported

UI fallback text:

“No SC2 Bank Folder path auto-detected, please select a folder”

Views

Simple View

Table View

Raw View

Raw View saves by re-signing, so it is a first-class editing mode

Map list behavior

searchable

scrollable

hide Blizzard maps

hide maps without bank files

Breadcrumb behavior

horizontally scrollable

pinned to available width

auto-scroll to end

scrollbar visually hidden

Top-right controls

Open in Folder

Tools

Settings

tooltips via fyne-tooltip

Tools menu

Unprotect external map file

Re-sign external bank

Editor polish

reset button restores original loaded values

same-value editing across multiple fields is desired

clean auto-validating inputs

game-specific simplified views later

Cross-platform / performance goals

minimal memory use

lazy loading

avoid duplicate representations where possible

atomic file writes

Library/API goals

GetSC2RootPath

GetBankFolderFromRoot

IsValidBankPath

bank model matching original StarBank concepts

deterministic ASCII ordering matching StringComparer.Ordinal