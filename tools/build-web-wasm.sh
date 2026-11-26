#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

bindgen_version=0.2.129
if ! command -v wasm-bindgen >/dev/null 2>&1 ||
	[ "$(wasm-bindgen --version)" != "wasm-bindgen $bindgen_version" ]; then
	printf 'Install matching WebAssembly bindings with cargo install --locked --version %s wasm-bindgen-cli\n' "$bindgen_version" >&2
	exit 1
fi

target=wasm32-unknown-unknown
cargo build --locked --release --target "$target" --package starbank-wasm --target-dir target
wasm-bindgen --target web --out-dir apps/starbank-web/src/generated \
	target/"$target"/release/starbank_wasm.wasm
