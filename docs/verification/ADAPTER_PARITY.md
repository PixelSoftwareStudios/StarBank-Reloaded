# Headless adapter parity

The fixture compares the native Rust editor protocol, the web build's release WASM and the CLI on the same synthetic UTF-8, BOM-marked UTF-16LE and BOM-marked UTF-16BE banks. It runs in Node without a browser or desktop window. The fixture uses CRLF, mixed quote style, Unicode names and comments, and an entity-escaped string beside the edited value.

## Run

Use the pinned Rust toolchain with the wasm32 target, wasm-bindgen 0.2.129, and the workspace Node version. From the repository root on a host with CLI exclusive output creation:

```sh
cargo build --locked --target-dir target -p starbank-cli --bin starbank
cargo build --locked --target-dir target -p starbank-editor --example parity_protocol
pnpm --filter starbank-web run wasm
node apps/starbank-web/test/adapter-parity.mjs
```

The test replays 34 identical protocol requests per encoding and compares complete JSON responses and byte attachments. It checks that grouped integer input above JavaScript's safe integer limit produces the same native bytes through structured editing and Raw XML text. Malformed Raw XML, grouped numbers in Raw XML, opposite byte order, and stale revisions leave the document unchanged. Undo and opening snapshots return the original bytes. Read-only draft checks compare ordered integer/fixed diagnostics, exact grouped values above 2^53, unchanged snapshots and raw bytes, stale revisions, and rejection of both empty and nonempty attachments. The CLI's unedited export, structured edit and signed copy agree with independently constructed bytes. Its dry-runs leave the directory and source unchanged, and an existing destination or stale patch fingerprint yields a typed conflict.

## Evidence

At `0157e0c`, all three encodings passed on macOS arm64 with Rust 1.100.0, wasm-bindgen 0.2.129 and Node 24.10.0. Repeating the comparison at `d0d3d9d` with pinned Node 24.22.3 reproduced all 69 replies and the same file digests. The expanded fixture passed 102 exact native/release-WASM replies on November 27, 2026 using Rust 1.100.0, wasm-bindgen 0.2.129 and pinned Node 24.22.3. The three historical signed-byte digests were unchanged. The independently computed legacy signature is `B19D9C042524092B16FDC91EC5D3BC6085D26383` for all three encodings.

| Bank encoding | Signed file SHA-256 |
|---|---|
| UTF-8 | `530f58020a62ab8d201fe084054bd7d55e297e13299452c1484a1ac9857ad4fe` |
| UTF-16LE | `2bc093ad01d2c655fc024dd500d27d71d52490ab1be0ae2c0136e21459a749f9` |
| UTF-16BE | `77cc65cb5ff8120be2c0c1d2d0f1e0d12c077c876fb9ee0c5bc05fe9284b0630` |

This evidence covers the native protocol and shipped WASM binding. The Tauri IPC, browser worker, Windows runtime and game-generated banks have separate qualification requirements.
