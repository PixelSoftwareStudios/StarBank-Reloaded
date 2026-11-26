# StarBank Reloaded | Architecture pack

The architecture baseline is dated 2026-11-26. See [TASKS.md](TASKS.md) for implementation progress and [GO_INVENTORY.md](docs/reference/GO_INVENTORY.md) for migration evidence.

StarBank Reloaded is a local-first, cross-platform StarCraft II bank editor with a planned Rust engine, Tauri desktop app, shared React UI and static WebAssembly-powered browser edition. The project targets feature parity with BlueRaja/Starbank v2.0 and retains the accepted Reloaded features from the earlier design.

## Read in this order

1. [AGENTS.md](AGENTS.md) — repository engineering rules.
2. [PLAN.md](PLAN.md) — product, scope and release gates.
3. [ARCHITECTURE.md](ARCHITECTURE.md) — workspace and ownership boundaries.
4. [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md) — authoritative feature register with stable IDs.
5. [TASKS.md](TASKS.md) — ordered, testable implementation work packages.
6. [docs/design/UX.md](docs/design/UX.md), [visual reference](docs/design/editor-reference.svg), [docs/design/EDITOR.md](docs/design/EDITOR.md) — visual and behavioral specifications.
7. [docs/design/BANK_FORMAT.md](docs/design/BANK_FORMAT.md), [docs/design/SIGNATURE_SPEC.md](docs/design/SIGNATURE_SPEC.md), [docs/design/DISCOVERY_MPQ.md](docs/design/DISCOVERY_MPQ.md), [docs/design/PERSISTENCE.md](docs/design/PERSISTENCE.md) — compatibility and safety design.
8. [docs/design/PLATFORMS.md](docs/design/PLATFORMS.md), [docs/design/PROFILES.md](docs/design/PROFILES.md), [docs/design/AGENT_API.md](docs/design/AGENT_API.md) — shared frontend, profiles and agent-facing access.
9. [docs/standards/EES-1.md](docs/standards/EES-1.md), [docs/standards/EVS-1.md](docs/standards/EVS-1.md), [docs/verification/TEST_PLAN.md](docs/verification/TEST_PLAN.md) — engineering and verification contracts.
10. [docs/TECH_STACK.md](docs/TECH_STACK.md), [docs/decisions/ADR_REGISTER.md](docs/decisions/ADR_REGISTER.md), [docs/reference/UPSTREAM.md](docs/reference/UPSTREAM.md), [docs/reference/CONVERSATION_DECISIONS.md](docs/reference/CONVERSATION_DECISIONS.md) — rationale, references and historical reconciliation.

## Go prototype archive

The Go/Fyne checkout is preserved under `original/`, with Git history at the root. [MIGRATION.md](MIGRATION.md) records the archive procedure. The live legacy plans under `original/docs/` and the companion snapshots under `reference/legacy-plans/` are superseded by the requirements and designs above.

## Verification

Run `python3 tools/validate_pack.py` to check the requirements registers, navigation links and archived file hashes. The [archive manifest](docs/reference/GO_ARCHIVE_MANIFEST.json) records the verified live bytes and the difference from the supplied snapshot.
