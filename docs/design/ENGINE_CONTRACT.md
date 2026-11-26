# Shared engine boundary

The editor owns portable commands, projections and diagnostics. Tauri and the WASM worker use the same Rust DTOs. The React client validates their shapes with Valibot. The CLI uses the same editor services and serializes versioned command results.

## Requests and revisions

A request carries `schema_version: 1`, a bounded correlation ID and an adjacently tagged `kind` and `payload` command. Session operations also carry the document ID and expected revision. Open and capability queries precede document creation. Reject unknown command variants, request fields and unsupported versions before mutation. Responses echo a valid correlation ID and include the observed revision when the request addressed an open session. Older clients can ignore additive response fields.

The editor advances a checked revision counter when canonical state changes. Revisions cross JSON as canonical unsigned decimal strings. The client compares equality and obtains the next revision from the editor. Bank value lexemes are also strings, preserving leading zeroes, decimal scale and values beyond the JavaScript safe integer range. The Rust engine validates bank types and interprets numeric syntax.

Section, key and value IDs identify occurrences within a document. Sorting or renaming retains these IDs. If replacing Raw XML makes identity ambiguous, send a fresh projection and invalidate the affected IDs. Ignore late responses from the previously selected document. A stale revision returns a typed error and a fresh projection can be requested. Correlation IDs route replies. Revision checks and single-use prepared operations guard mutations.

Opening bytes, the current document and the last verified saved bytes are separate. Save acknowledgements identify the prepared revision and operation. A successful acknowledgement updates only the corresponding saved checkpoint and leaves later edits dirty. Copy and dry-run leave the saved checkpoint and filesystem untouched.

## File and platform access

The engine opens owned bytes and prepares validated output. Each platform adapter owns file access, authorization and capabilities. Native adapters retain `PathBuf` and `OsString` and issue scoped opaque IDs to the client. Display paths are presentation labels. Tauri commands must bind issued IDs to the owning window or document and enforce authorization on each operation. Register explicit app command permissions for the local editor window. [Tauri capabilities](https://v2.tauri.app/security/capabilities/)

Browser file inputs and drop provide readable bytes. Export as a download is the baseline. Optional in-place saving requires a supported writable handle, a current user grant and a fresh content check. The CLI takes OS-native paths from command arguments. Durable patch files identify occurrences within an expected base fingerprint. Session occurrence IDs expire when the document closes. [Browser file-handle permissions](https://wicg.github.io/file-system-access/#local-file-system-permissions)

Tauri uses typed Serde control messages. The initial WASM worker sends JSON control messages and transfers bounded byte buffers separately. This avoids coercing file bytes into text and allows the Rust JSON decoder to reject unpaired surrogates in string fields. Product bounds cover file size, command operations and worker queues. [Tauri command transport](https://v2.tauri.app/develop/calling-rust/) · [wasm-bindgen data transport](https://wasm-bindgen.github.io/wasm-bindgen/reference/arbitrary-data-with-serde.html)

## Acceptance cases

| Concern | Check |
|---|---|
| Transport parity | Compare native, WASM and CLI projections of identical synthetic bytes, including ordered duplicate names, exact values, signature status and diagnostics. |
| Version and numbers | Reject unknown versions, operations and request fields, numeric JSON revisions and lexemes, malformed or overflowing revisions and invalid Unicode. Preserve `9007199254740993.50` exactly. |
| Stale requests | Apply at revision R, then replay R and assert that state stays at the new revision. Change document during a read and drop the late reply. |
| Save acknowledgement | Prepare R and edit to R+1 before completion. Verify that the new edit stays dirty and a replayed acknowledgement cannot mutate state. |
| Authorization | Reject cross-window or expired handles, exercise denied browser permissions, and operate on native paths whose display form loses bytes. |
| Limits | Reject excess bytes, operation counts and queue depth. Verify worker byte transfer, stale-result suppression and worker-init failure. |
