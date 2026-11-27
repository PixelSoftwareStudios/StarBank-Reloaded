# Production browser timing on synthetic banks

A clean archive of `aaf5b58c5f70a04e4a931267b7c8e5845b5703f1` produced the Vite assets on November 27, 2026. The run used an Apple M1 Pro with macOS 14.7.2, headless Chromium 159.0.8154.6, Playwright 1.65.1, and an 850×500 viewport. Node 24.22.3, pnpm 12.23.0, Rust 1.100.0 and wasm-bindgen 0.2.129 matched the project pins. The script ran five discarded warmups, then 25 timed pairs. The production asset fingerprint was `47f43e61c069ea94b2a3520057e24558696cb8727567a1e8669c37f4dcc1f7df`.

| Browser operation | Fixture | Median (ms) | p95 (ms) |
| --- | --- | ---: | ---: |
| Navigate to enabled bank picker, fresh context | Empty editor | 75.5 | 83.7 |
| Navigate to enabled bank picker, same-context warm cache | Empty editor | 71.3 | 76.4 |
| File input change to first projected table row | 32 values, 1,383 bytes | 24.2 | 26.9 |
| File input change to first projected table row | 10,000 values, 457,839 bytes | 72.2 | 83.1 |
| Scroll to row 9,000 and mount its input | 10,000 values | 8.6 | 16.0 |
| Apply one staged exact integer edit | 32 values | 51.2 | 56.4 |
| Apply one staged exact integer edit at row 9,000 | 10,000 values | 67.9 | 71.2 |

Each fresh Chromium context requested nine static assets, including one WASM resource. The second navigation used cached assets in every measured pair. The server sent HTML with `no-store` and retained hashed assets for a year. It served uncompressed production bytes from memory over loopback. Chromium was already running and the OS file cache was left in its normal state.

The measurements use the browser's monotonic clock. Startup ends when the real engine enables the file input. The file-open interval starts with the local file input's change event, after automation uploads the synthetic file. Scroll starts at the viewport's scroll assignment. Apply starts at the button's actual click event and ends when the draft clears, the picker re-enables and the exact accepted integer is displayed. Each trial alternated the order of the two warmed-app fixture runs. p95 uses nearest rank.

The large table mounted 12 data rows initially and 20 around row 9,000 in all 25 measurements. The small table mounted all 32. These local DOM timings cover the measured Chromium and inputs. Browser launch, physical screen paint, remote asset delivery, other engines, and process-tree CPU or memory are outside the measured intervals.

## Rust draft validation run

The production build of `028224f` passed the same five warmups and 25 timed pairs with the pins and host above. The asset fingerprint was `a9d9e7d629b49befef6d1dd8a652361ca02d3cc60e08caeb341f2e7c30cc7224`. The updated harness also measures the input event to the enabled Apply button, including the 150 ms debounce and the read-only Rust reply. Apply continues to measure only the subsequent click-to-accepted-revision interval.

| Browser operation | Fixture | Median (ms) | p95 (ms) |
| --- | --- | ---: | ---: |
| Navigate to enabled bank picker, fresh context | Empty editor | 67.6 | 72.4 |
| Navigate to enabled bank picker, warm cache | Empty editor | 65.2 | 81.8 |
| File input change to first projected table row | 32 values | 14.3 | 16.0 |
| File input change to first projected table row | 10,000 values | 63.0 | 84.7 |
| Scroll to row 9,000 and mount its input | 10,000 values | 4.4 | 15.9 |
| Input event to validated, enabled Apply | 32 values | 155.4 | 156.6 |
| Input event to validated, enabled Apply | 10,000 values | 156.5 | 157.7 |
| Apply one staged exact integer edit | 32 values | 54.0 | 58.4 |
| Apply one staged exact integer edit at row 9,000 | 10,000 values | 70.5 | 75.8 |

All trials retained exact values above 2^53, cold and warm request-count checks, and zero off-origin requests or page errors. These observations do not establish owner-approved performance budgets or installed native performance.

## Reproduce

Build with the project pins and run the benchmark using the workspace's Playwright dependency.

```sh
pnpm install --frozen-lockfile
pnpm build
pnpm --filter starbank-web exec playwright install chromium
pnpm --filter starbank-web exec node test/browser-perf.mjs > samples.jsonl
```

The benchmark checks cold versus warm request counts, exact edit completion, absence of page errors, and locality of HTTP requests. It emits one environment record and 25 samples. To measure the historical revision above, build it separately and pass its absolute `apps/starbank-web/dist` path as the script's argument. [Benchmark source](../../apps/starbank-web/test/browser-perf.mjs)
