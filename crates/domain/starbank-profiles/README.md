# Declarative bank profiles

This crate implements profile matching and edit proposals for REQ-PROF-01 and REQ-PROF-02. The version 1 format is illustrated by the synthetic [training definition](tests/fixtures/training.json) and its [bank](tests/fixtures/training.sc2bank).

- An imported profile requires a unique ID and the exact author and bank stem. Optional map version, SHA-256 fingerprint and bank field predicates add identity checks. Equal-confidence matches return the candidate IDs for explicit resolution.
- Addresses select section and key by exact name and can select a named value child. An omitted occurrence requires a unique match. An explicit occurrence counts matching names from zero in source order. An omitted child requires a key with exactly one child.
- Each field declares a numeric, checkbox or text mapping. The numeric strategy accepts canonical integer and fixed-point strings with exact bounds, up to 18 fractional places and checked signed 128-bit mantissas. The entry point supplies canonical numeric input after parsing the user's draft.
- Rules have one source and one target, and calculate `source × multiplier + addend` at a shared declared scale. Each target has one writer. The graph is acyclic, and dependency targets are read-only in the projection.
- A binding refers to one document snapshot. The engine applies the entire preview under the snapshot's revision guard using its normal value transaction. Rebind after any accepted edit, undo or document switch.
- The legacy importer accepts the archived YAML mapping with an explicit profile ID and author. It uses the original checkbox lexemes `1` and `0`, case-insensitive stems and unique section/key/value bindings.

The included fixtures are synthetic. The application still needs independently verified map definitions and session integration.
