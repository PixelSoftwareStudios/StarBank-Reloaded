# StarBank Reloaded
A StarCraft II bank file editor

## Exposition

[StarBank](https://github.com/BlueRaja/Starbank) is a great piece of software, but it's been around since 2012 and it hasn't been meaningfully updated in 9 years. It's built for Windows only and is quite dated.

StarBank Reloaded aims to be a cross-platform, modern version of StarBank, for the 15 or so SC2 players still around today.

## Installation

Binaries available in the Releases tab or run `just build`

## Development

Use the Rust toolchain pinned in `rust-toolchain.toml`, Node 24.22.3, and pnpm 12.23.0. StarBank Web builds its local editor worker with wasm-bindgen CLI 0.2.129.

```sh
cargo install --locked --version 0.2.129 wasm-bindgen-cli
pnpm install --frozen-lockfile
pnpm dev
```

StarBank Web opens individual banks or a local bank list from selected files and folders. It edits Table and Raw views and offers Simple View for matching local profiles. Structural edits, targeted reverts, signed downloads and byte-exact backups of the opening file work offline after the editor loads. The shared engine preserves UTF-8 and BOM-marked UTF-16 bank files.

Build the desktop webview assets, then run the desktop app from the repository root.

```sh
pnpm --filter starbank-desktop run build:webview
cargo run --locked
```

Run the Tauri desktop development host or the command-line editor from the same checkout.

```sh
pnpm --filter starbank-desktop run dev
cargo run --locked -p starbank-cli -- --help
```

The desktop app opens local banks and browses banks from an authorized SC2 folder. Verified overwrite and restore are available on qualified macOS APFS targets. macOS and Linux support create-only signed copies and confirmation-gated batch backups. Windows supports editing and create-only Save As, export and external signed copies in qualified local NTFS directories below the drive root; Windows overwrite, desktop backups, restore and save-on-close remain unavailable. The CLI supports JSON inspection, validation, diff, discovery, dry-run, patching, signing, backups and read-only map metadata inspection. [CLI guide](apps/starbank-cli/README.md) · [Verified platform scope](docs/verification/IMPLEMENTATION.md) · [Local packages](docs/verification/DISTRIBUTION.md)

`just --list` lists the local test, build, and packaging recipes.

See the [architecture](docs/architecture.md) and the [current test evidence](docs/verification/IMPLEMENTATION.md).
