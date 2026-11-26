# ARCHITECTURE.md

## Overview

StarBank Reloaded is a cross-platform StarCraft II bank editor written
in Go.\
The architecture separates the **GUI layer** from the **bank editing
engine** so the core logic can be reused by CLI tools or other
applications.

Main goals:

-   Cross platform (Windows, macOS, Linux)
-   Minimal memory usage
-   Clean separation of UI and logic
-   Exact compatibility with SC2 bank signature algorithm
-   Stable and maintainable Go codebase

------------------------------------------------------------------------

# Project Structure

cmd/starbank/ main.go GUI entrypoint (Fyne)

pkg/bankeditor-lib/ model.go parser.go serializer.go signature.go
sorting.go paths.go writer.go

------------------------------------------------------------------------

# Core Library (`bankeditor-lib`)

The library handles all functionality related to SC2 bank files.

Responsibilities:

-   Parsing bank XML
-   Editing bank data
-   Sorting sections and keys
-   Generating signatures
-   Writing bank files safely
-   Resolving SC2 installation paths

------------------------------------------------------------------------

# Data Model

model.go defines the primary structures.

Bank ├ BankInfo ├ Sections \[\]\*Section ├ Signature string └ Version
string

Section ├ Name string └ Keys \[\]\*Key

Key ├ Name string ├ Type string ├ Value string └ Section \*Section

BankInfo ├ BankPath ├ Name ├ DateModified ├ PlayerNumber └ AuthorNumber

------------------------------------------------------------------------

# Parsing Layer

parser.go reads bank XML files and builds the Bank model.

Responsibilities:

-   Parse XML into Go structs
-   Preserve ordering of sections and keys
-   Preserve value types
-   Maintain reference relationships

Functions:

ParseBankFile(path string) (*Bank, error) ParseBankXML(xml string)
(*Bank, error)

------------------------------------------------------------------------

# Serialization Layer

serializer.go converts the Bank struct back into XML.

Responsibilities:

-   Maintain valid SC2 XML structure
-   Ensure deterministic ordering
-   Support raw editing workflows

Functions:

SerializeBank(bank \*Bank) (\[\]byte, error)

------------------------------------------------------------------------

# Signature Engine

signature.go replicates the signature algorithm used by SC2.

Process:

1.  Order sections by ASCII name
2.  Order keys by ASCII name
3.  Concatenate values according to StarBank logic
4.  Compute SHA1 hash
5.  Store uppercase hex string in `<Signature>`

Functions:

GenerateSignature(bank *Bank) string UpdateSignature(bank *Bank) error

------------------------------------------------------------------------

# Sorting Utilities

sorting.go ensures deterministic ordering.

Functions:

OrderSectionsByNameASCII(\[\]*Section) \[\]*Section
OrderKeysByNameASCII(\[\]*Key) \[\]*Key

Requirements:

-   Stable sort
-   Returns new slice
-   Uses ASCII comparison

------------------------------------------------------------------------

# File Writer

writer.go handles writing banks to disk.

Responsibilities:

-   Atomic writes
-   Optional backups
-   Preserve file permissions when possible

Functions:

WriteBank(path string, bank *Bank) error WriteBankWithBackup(path
string, bank *Bank) error

Write strategy:

temp file write sync replace original

------------------------------------------------------------------------

# Path Detection

paths.go finds the SC2 installation and bank folder.

Functions:

GetSC2RootPath() GetBankFolderFromRoot() IsValidBankPath()

Supported platforms:

Windows macOS Linux (stub)

------------------------------------------------------------------------

# GUI Layer

The GUI is implemented in `cmd/starbank` using Fyne.

Responsibilities:

-   Display map list
-   Provide editing interfaces
-   Handle user input
-   Call library functions

The GUI should contain **no core bank logic**.

------------------------------------------------------------------------

# Editor Views

Simple View

-   Friendly editing of common values

Table View

-   Structured section/key editing

Raw View

-   Direct XML editing

Saving from any view triggers signature regeneration.

------------------------------------------------------------------------

# Performance Considerations

Avoid unnecessary allocations Clone slices only when sorting Lazy load
bank files Avoid repeated XML parsing

------------------------------------------------------------------------

# Future Expansion

CLI editor Batch editing Bank diff viewer Plugin support
