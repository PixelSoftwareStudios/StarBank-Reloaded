# Architecture decision register

Accepted = explicit direction or established standard. Proposed = recommendation subject to validation/owner review. Pending = evidence needed before commit to implementation. Record future ADRs with context, alternatives, rationale, consequences and test obligations.

| ADR | Status | Decision |
|---|---|---|
| 001 | Accepted | Archive Go snapshot under original; Rust workspace, Tauri desktop, shared web/WASM, CLI. No Go runtime requirement. |
| 002 | Accepted | React/TS/Vite/Tailwind 4/HeroUI/Valibot/Lucide; no Zod/shadcn default. Evaluate HeroUI table before introducing TanStack Table. |
| 003 | Accepted | Preserve approved compact UI and exact labels/removed features. No unrelated layout changes. |
| 004 | Accepted | Native/WASM authoritative Rust sessions and versioned control messages with exact decimal-string revisions. Use a separate binary payload path for file contents. [Contract](../design/ENGINE_CONTRACT.md) |
| 005 | Proposed | Lossless XML/source model with nested values, stable IDs; no flat lossy deserialization or locale signing. |
| 006 | Accepted | First valid SC2 root, all account banks, manual override, Linux auto detection stub; later Proton/Lutris. |
| 007 | Pending | MPQ backend: wow-mpq vs stormlib-rs vs StormLib fallback; real native read/write and WASM feasibility gate. |
| 008 | Pending | Qualify a native replacement adapter for each supported filesystem. The atomic-write-file 0.3.1 spike lost ACLs and extended attributes. [Evidence](../verification/BOOTSTRAP.md) |
| 009 | Proposed | Keep opening snapshot across saves; separate last-saved revert. Default Save As switches active document; export copy option. Auto backup first overwrite per session default + explicit timestamp backup. Owner can adjust in Settings. |
| 010 | Accepted | Rust 1.100.0 verified on macOS arm64 and for wasm32. Apply EES-1 and EVS-1 to new code. [Evidence](../verification/BOOTSTRAP.md) |
| 011 | Pending | MIT preferred for new code. File-level provenance, permissions and release-license approval remain open. Keep the upstream source dump local pending review. |
| 012 | Proposed | Agent CLI structured JSON/dry-run, no separate daemon; static hosting Cloudflare Pages candidate. |
| 013 | Proposed | Declarative profiles only, no arbitrary code execution; match confidence and dependent atomic updates. |
| 014 | Proposed | Preserve Tools map repair only for authorized offline copy, no runtime/memory tools. Explicit limitations if backend cannot repair safely. |

Do not treat any Pending choice as already implemented or verified. Changes to owner-approved UI/stack require explicit approval, not an incidental refactor.
