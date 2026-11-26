# CLI, machine-readable and agent-first access

The CLI is a thin entrypoint under `apps/starbank-cli`, built from the same editor services, not a second parser/signer. No running GUI required. Explicitly scoped local files only; no backend HTTP service, game process, broad filesystem write command, shell or arbitrary code execution. Proposed interface (names subject to contract review, not implemented):

```text
starbank inspect FILE [--json]
starbank validate FILE [--json]
starbank diff BEFORE AFTER [--json]
starbank sign FILE --author HANDLE --player HANDLE --bank-name NAME [--dry-run] [--output PATH]
starbank edit FILE --patch PATCH [--dry-run] [--output PATH] [--backup]
starbank export FILE --output PATH
starbank discover [--root PATH] [--json]
starbank backup list|create|restore ...
starbank map inspect FILE [--json]
```

Do not make destructive overwrite implicit in CLI: require explicit `--in-place` or overwrite confirmation and backup policy; `--dry-run` emits exact intended operations/diff and signing status with zero writes; explicit context or reliable path inference, no silent default identity. Typed patch operations, expected fingerprint/revision, stable occurrence addresses, multi-edit transaction; dry-run uses same prepare path as commit. Machine-readable versioned JSON for data/errors/diagnostics, stdout reserved for data, stderr logs, stable documented exit codes (success, validation/context, IO, conflict, uncertain, unsupported), no personal path/value logging by default. Large data streaming/pagination where applicable. CLI map commands subject to same authorized-copy policy. Documentation and an agent skill with command help, schemas, examples on synthetic fixture, no private data or destructive defaults. Contract tests compare CLI, native IPC and WASM semantic results, no duplicated business rules. Optional app APIs beyond typed local commands only if later approved.
