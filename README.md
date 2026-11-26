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

StarBank Web opens local banks, edits Table and Raw views, and exports an exact or re-signed copy. After the app and its editor worker have loaded, these operations work offline.

Run the Tauri desktop development host or the command-line editor from the same checkout.

```sh
pnpm --filter @starbank/desktop run dev
cargo run --locked -p starbank-cli -- --help
```

The desktop host opens local banks in the shared editor and supports create-only export on macOS and Linux. The CLI supports JSON inspection, validation, diff, discovery, dry-run, patching, signing and backups. See the [CLI guide](apps/starbank-cli/README.md) and [verified platform scope](docs/verification/IMPLEMENTATION.md).

New code licensing remains pending provenance review. The archived Go prototype retains its original [license notice](original/LICENSE).
