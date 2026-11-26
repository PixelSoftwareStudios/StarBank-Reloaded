# StarBank Reloaded

A local-first StarCraft II bank editor in development. The roadmap covers a Rust engine, Tauri desktop app, shared React frontend and browser edition.

Start with [START_HERE.md](START_HERE.md) for the architecture and work packages. The live Go prototype, its development configs and legacy plans are archived under [original/](original/). The migration evidence is in the [Go inventory](docs/reference/GO_INVENTORY.md).

## Development

Use the Rust toolchain pinned in `rust-toolchain.toml`. For the web shell, use Node 24.22.3 and pnpm 12.23.0.

```sh
pnpm install --frozen-lockfile
pnpm dev
```

The shell currently shows the empty workspace. Bank editing awaits the engine and file adapter.

New code licensing remains pending provenance review. The archived Go prototype retains its original [license notice](original/LICENSE).
