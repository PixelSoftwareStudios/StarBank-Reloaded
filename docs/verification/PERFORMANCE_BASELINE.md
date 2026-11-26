# Synthetic engine and map index performance

The first baseline was measured on November 26, 2026, using repository revision `c078263` with the benchmark harness applied. The host was an Apple M1 Pro with 32 GiB of memory, macOS 14.7.2, and Rust 1.100.0 (`0ef1bc386`). These measurements cover local synthetic operations for REQ-PERF-01 and resource-bound checks for REQ-QA-01.

## Reproduce

```sh
cargo bench --locked -p starbank-editor -p starbank-signature -p starbank-map-index --bench workloads
```

Use the workspace Rust pin and Cargo's optimized benchmark profile. The harness runs five warmups and 25 timed calls per case. It reports median and nearest-rank p95 in nanoseconds and checks each operation's result after its timer stops. Fixture generation, assertions, and destruction of the timed result and state fall outside the measured interval. When comparing revisions, build each in its own target directory and record the executable hashes.

- The editor and signature suites generate identical unsigned banks with 32 keys (1,767 bytes) and 8,192 keys (435,611 bytes). Bulk edits touch 32 and 128 keys respectively. The editor validates exact numeric text, stable revision behavior, and rejection of a raw input one byte beyond its cap. The signing suite verifies successful output and checks that a failed preparation at the output cap leaves the input bytes intact. [Editor workloads](../../crates/domain/starbank-editor/benches/workloads.rs) · [Signature workloads](../../crates/domain/starbank-signature/benches/workloads.rs)
- The map suite parses 1 KiB with four references and 8 MiB with 1,000 repeated references to one synthetic bank name. Archive scans reuse four or 256 files made from three committed synthetic fixtures. An uncached scan disables the application metadata cache. A warm scan starts with every metadata record available. Both use repeated file access at the OS level. The count-limit case verifies that refusal preserves the persisted cache bytes. [Map workloads](../../crates/discovery/starbank-map-index/benches/workloads.rs) · [Archive fixture provenance](../../crates/discovery/starbank-map-index/tests/fixtures/README.md)

## Baseline observations

These are selected cases from one optimized local run. The environment also ran other tasks, so tail timings varied between invocations.

| Operation | Fixture | Median (ms) | p95 (ms) |
| --- | --- | ---: | ---: |
| Open editor session | 8,192 keys | 6.585 | 16.170 |
| Apply exact bulk edit | 8,192 keys, 128 edits | 18.171 | 37.006 |
| Compute opening diff | 8,192 keys, 128 edits | 11.785 | 16.604 |
| Prepare signed editor export | 8,192 keys, 128 edits | 10.050 | 13.189 |
| Compute bank signature | 8,192 keys | 0.617 | 0.709 |
| Re-sign, reparse and verify | 8,192 keys | 8.705 | 11.416 |
| Parse static map metadata | 8 MiB, 1,000 repeated references | 7.233 | 8.337 |
| Scan archives without metadata cache | 256 files | 9.058 | 16.722 |
| Scan archives with warm metadata cache | 256 files, 256 hits | 59.076 | 215.589 |

Build with the same Cargo command plus `--no-run --message-format=json` to identify the individual benchmark executables. On macOS, run each executable through `/usr/bin/time -l` to measure its peak resident set and memory footprint. This second run measured the whole benchmark process, including fixture setup and verification.

| Benchmark process | Maximum RSS (bytes) | Peak memory footprint (bytes) |
| --- | ---: | ---: |
| Editor | 118,734,848 | 46,319,040 |
| Signature | 75,005,952 | 41,125,312 |
| Map index | 17,170,432 | 15,074,688 |

## Unchanged metadata cache

A warmed scan that reuses an identical metadata image now keeps the file in place. A regression test fixed the existing cache's modification time before a full-hit scan. The old scan replaced it, and the revised scan preserved it. A missing, invalid, or changed cache still goes through the existing save path. [Cache test](../../crates/discovery/starbank-map-index/tests/index.rs)

Three paired runs used distinct optimized executables and alternated their run order. The table reports the median of the three per-run medians on the same synthetic inputs and host.

| Workload | Before (ms) | After (ms) |
| --- | ---: | ---: |
| Warm metadata cache, four hits | 10.320 | 0.273 |
| Warm metadata cache, 256 hits | 67.033 | 7.563 |
| Uncached control, four files | 0.369 | 0.377 |
| Uncached control, 256 files | 10.188 | 10.731 |

## Open measurements

Desktop and browser startup, idle and peak memory, UI interaction latency, real SC2 corpora, cross-platform measurements, and approved performance budgets remain open.
