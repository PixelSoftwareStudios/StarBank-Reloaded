# StarBank Reloaded

A local-first StarCraft II bank editor in development. The roadmap covers a Rust engine, Tauri desktop app, shared React frontend and browser edition.

Start with [START_HERE.md](START_HERE.md) for the architecture and work packages. The live Go prototype, its development configs and legacy plans are archived under [original/](original/). The migration evidence is in the [Go inventory](docs/reference/GO_INVENTORY.md).

## Development

Use the Rust toolchain pinned in `rust-toolchain.toml`, Node 24.22.3, and pnpm 12.23.0. StarBank Web builds its local editor worker with wasm-bindgen CLI 0.2.129.

```sh
cargo install --locked --version 0.2.129 wasm-bindgen-cli
pnpm install --frozen-lockfile
pnpm dev
```

StarBank Web opens individual banks or a local bank list from selected files and folders. It edits Table and Raw views and offers Simple View for matching local profiles. Structural edits, targeted reverts, signed downloads and byte-exact backups of the opening file work offline after the editor loads. The shared engine preserves UTF-8 and BOM-marked UTF-16 bank files.

Run the Tauri desktop development host or the command-line editor from the same checkout.

```sh
pnpm --filter starbank-desktop run dev
cargo run --locked -p starbank-cli -- --help
```

The desktop app opens local banks and browses banks from an authorized SC2 folder. Verified overwrite and restore are available on qualified macOS APFS targets. macOS and Linux support create-only signed copies and confirmation-gated batch backups. Windows supports reading banks and editing in memory. The CLI supports JSON inspection, validation, diff, discovery, dry-run, patching, signing, backups and read-only map metadata inspection. [CLI guide](apps/starbank-cli/README.md) · [Verified platform scope](docs/verification/IMPLEMENTATION.md) · [Local packages](docs/verification/DISTRIBUTION.md)

`just --list` lists the local test, build, and packaging recipes.

New code licensing remains pending provenance review. The archived Go prototype retains its original [license notice](original/LICENSE).
