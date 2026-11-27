default:
    @just --list

fmt:
    cargo fmt --all -- --check

lint: desktop-assets
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo run --locked -p starbank-source-check
    pnpm lint

test: desktop-assets
    cargo test --locked --workspace --all-targets
    cargo test --locked --workspace --doc
    pnpm test

wasm:
    pnpm --filter starbank-web run wasm

desktop-assets:
    pnpm --filter starbank-desktop run build:webview

build: desktop-assets
    cargo build --locked --workspace
    pnpm build

dev-web:
    pnpm --filter starbank-web run dev

dev-desktop:
    pnpm --filter starbank-desktop run dev

package:
    pnpm --filter starbank-desktop run build -- --locked
