# Headless verification of authorized SC2 fixtures

The private corpus runner tests copied banks and maps named in a local manifest.
Store the manifest and fixture files outside the checkout. Use only inputs
authorized for local verification, such as a capture from an owner-authored
test map with a dedicated test identity. The game capture and any later
in-game acceptance check happen separately.

Keep exact emitted bytes, the original bank basename and the original
signing context together. Changing a handle, name or payload can change the
signature. Record the acquisition or capture date and steps, authorizing
owner or permission reference, and the exact copied byte count and SHA-256.
For a documented game capture, also record the game build, platform, and map
revision. Derive expectations from independently read XML or archive
metadata, the authored map, the captured signature, or another independent
reference. A StarBank Reloaded run cannot supply its own expected results.
A synthetic or re-signed anonymized specimen remains synthetic. The
manifest declares provenance. The runner checks required fields and
fingerprints.

## Run

Build the adapters from the same revision using the pinned Rust and Node
versions and matching wasm-bindgen.

```sh
cargo build --locked --target-dir target -p starbank-cli --bin starbank
cargo build --locked --target-dir target -p starbank-editor --example parity_protocol
pnpm --filter starbank-web run wasm

node docs/verification/scripts/private-sc2-corpus.mjs /path/to/private/manifest.json \
  target/debug/examples/parity_protocol target/debug/starbank \
  apps/starbank-web/src/generated/starbank_wasm.js \
  apps/starbank-web/src/generated/starbank_wasm_bg.wasm
```

The runner opens only listed regular files beneath the manifest directory.
It refuses traversals out of that scope and paths that resolve through
aliases. The CLI receives a single listed archive path, with indexing cache
disabled. It only reads that archive. Signing context and bank bytes pass
through the native checker’s standard input. Child output stays in memory.
The summary contains case positions, fixed diagnostics and counts
by declared origin and type. It emits no input paths, names, digests, account
handles, map metadata or bank contents. A failure exits with status 1.
The runner checks source hashes before and after successful cases.
There is no desktop window, game process, browser or network service.

## Manifest version 1

The manifest root has `schemaVersion: 1` and a nonempty `cases` array.
Each record provides `path`, `kind` (`bank` or `map`), exact
`byteLength`, lowercase `sha256`, `source` and `expected`.
Paths are relative to the manifest. The `source` object requires
`origin`, `capture`, `authorization` and `expectationSource`. The
nonempty `capture` records fixture preparation, acquisition, or capture
and exact-copy steps. Use the origin that matches the available evidence.

- `synthetic` covers authored, generated, and re-signed anonymized fixtures.
- `owner_file` covers an existing authorized owner-supplied local bank or
  map whose original game capture details are unavailable. Record how the
  owner provided it and how the verification copy was made. The runner
  counts these files separately from documented game captures.
- `game` covers documented captures with nonempty `gameBuild`, `platform`
  and `mapRevision` in addition to the shared source fields.

Keep sensitive values local.

Readable banks specify a complete `context` with `author`, `player`
and `bank_name`, or explicit `null` when checking without context.
Specify expected `encoding` (`utf-8`, `utf-16le` or `utf-16be`) and
`signatureStatus` (`unsigned`, `valid`, `invalid`, `malformed`,
`unsupported` or `missing_context`). The ordered `projection` nests
section, key and value arrays. In the synthetic format fixture
`ordinary.sc2bank`, the independent expectation is

```json
{
  "encoding": "utf-8",
  "signatureStatus": "unsigned",
  "projection": [
    ["Fixture", [
      ["Count", [["Value", "int", "9007199254740993"]]],
      ["Rate", [["Value", "fixed", "-0.1250"]]],
      ["Label", [["Value", "string", "A & B"]]]
    ]]
  ]
}
```

Names on sections or keys and kind or lexeme on values may be `null`
when absent. Preserve repeated names and original order. For an expected
parse refusal, use `{"openError":"invalid_document"}`. If an independent
expected signed output exists, give its exact lowercase `signedSha256`
along with the signing context.

The bank check opens the input, checks the expected projection, encoding
and signature state, exports without modification, and reparses the exact
exported bytes. An optional signed output must match its independent hash
and verify after reparse. It compares native and release WASM control
replies and byte attachments.

Maps specify an `expected` object with `exitStatus`, ordered
`issueCodes`, and `metadata`. Use `null` metadata for a rejected archive.
For example, the repository's synthetic `alpha.s2ma` expects

```json
{
  "exitStatus": 0,
  "issueCodes": [],
  "metadata": {
    "title": "Fixture Alpha",
    "titleSource": "script",
    "author": "Blizzard Entertainment",
    "bankNames": ["second", "shared"],
    "hasScript": true,
    "declaredBlizzard": true
  }
}
```

A map fixture can represent an authored `.SC2Map`, `.SC2Mod` or cached
`.s2ma`. Use independently observed archive members and source metadata
to prepare the expectations. The map check covers product metadata and
refusal behavior. Exact MPQ member extraction against StormLib and map
repair qualification require their own checks.

## Current evidence

The committed and documented scratch inputs are synthetic or source
references. The archived Go examples lack game capture provenance. Public
upstream issue excerpts and the contributor’s signed-bank totals do not
supply an authorized game corpus. On macOS 14.7.2 arm64 with Rust 1.100.0,
wasm-bindgen 0.2.129 and Node 24.10.0, the synthetic smoke passed six bank
and two map cases. Native and release WASM verification passed for signed, unsigned, malformed, UTF-8 and both
BOM-marked UTF-16 bank inputs. The map checks passed for a readable
`.s2ma` and a malformed archive. Six deliberate failures covered wrong context, a false hash,
a path outside scope, a symlink, missing game provenance and a metadata
mismatch. Their diagnostics remained redacted. Zero game banks and zero game maps were verified.

An owner-authorized private capture remains necessary for the real-file
parts of REQ-FMT-01, REQ-SIG-01 and REQ-MPQ-01. The runner can check local
outputs after capture. In-game acceptance for REQ-SIG-03 also needs a
separately authorized game-side verification.

[Format fixtures](../../crates/formats/sc2bank/tests/fixtures/README.md) ·
[Map fixtures](../../crates/discovery/starbank-map-index/tests/fixtures/README.md) ·
[MPQ extraction and copy evidence](../../crates/adapters/starbank-mpq/verification/MAP_EXPORT.md)
