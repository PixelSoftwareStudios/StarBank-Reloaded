# macOS desktop startup and native host resource sample

Measured on 2026-11-27 from StarBank Reloaded revision
`e2cf03533264146e23815f38f1353c2d5c829555`, using macOS 14.7.2 build
23H311 on a 10-core Apple M1 Pro MacBookPro18,1 with 32 GiB RAM. The arm64
native executable used Cargo's optimized release profile, Rust 1.100.0,
`custom-protocol`, and the lockfile. Production Vite 8.3.8 assets were built
with Node 24.22.3 and pnpm 12.23.0. The measured executable was 15,242,272
bytes, SHA-256 `9ecec7cb5baa234dfb2e76d5ff15377743424b882a0e0a6b712753742a4c7310`.
The production entry asset `index-BqT3nIS3.js` had SHA-256
`be08c5f226518fc0cb5568e07cc3b8d736c5d26b9b73449b915a86cc50cd36e8`.

The scratch build redirects settings, legacy settings, inventory, archive
scan, cache and backup paths to a new directory per trial. It uses a
nonpersistent WKWebView data store and a separate bundle identifier with
document associations removed. The release feature set and production
renderer source are unchanged. The runner launches the executable directly
in the interactive desktop session. This sample covers direct executable
launch and fresh app processes on a running OS with unflushed system caches.

## Timings

Times are medians (minimum–maximum) in milliseconds across five fresh
processes per scenario. The runner alternated scenarios and excluded one
initial sweep of all three cases.

| Synthetic scenario    | File bytes | Launch → on-screen window | Launch → enabled UI action | Confirm Open → section observable |
| --------------------- | ---------: | ------------------------: | -------------------------: | --------------------------------: |
| Empty editor          |          0 |             220 (218–410) |              511 (432–757) |                                 — |
| 32 integer values     |      1,557 |             401 (225–677) |            659 (547–1,155) |                      126 (88–170) |
| 10,000 integer values |    488,953 |             269 (221–729) |            614 (506–1,223) |                      130 (85–181) |

The launcher starts a monotonic timer immediately before `Process.run()`.
It polls for an on-screen Quartz layer-zero window owned by the launched
PID. It then polls that PID's accessibility tree for an enabled **Open
Bank…** button (empty case) or native-shell startup confirmation **Open**
button (file cases). In file cases it invokes the actual button with
`AXPress` and waits for the synthetic `PerfProbe` section in the production
editor accessibility tree. The 25 ms poll interval and AX query cost are
included. Confirmation timing includes the native open, parse, application
transport, and renderer work up to that accessible section. Quartz
visibility and AX readiness are separately reported observables.
[Apple window list API](https://developer.apple.com/documentation/coregraphics/cgwindowlistcopywindowinfo(_:_:))

## App host only

The launcher waits one second after UI readiness or after the synthetic
section appears. It takes seven readings approximately 500 ms apart using
`proc_pid_rusage(RUSAGE_INFO_V4)` on the specific executable PID. The idle
windows lasted 3.011–3.052 seconds. RSS and physical footprint are medians
of the five per-trial medians, with the range across all 35 readings in
parentheses. Maximum footprint is the kernel's lifetime physical-footprint
high-water mark as of the idle interval's end. Its range and the CPU ranges
are across the five trials.

| Synthetic scenario |    Idle RSS, MiB | Idle physical footprint, MiB | Lifetime max physical footprint, MiB | Idle CPU time, ms | Idle CPU, % of one core |
| ------------------ | ---------------: | ---------------------------: | -----------------------------------: | ----------------: | ----------------------: |
| Empty editor       | 67.1 (65.5–78.3) |             23.7 (21.6–25.8) |                     28.2 (27.4–29.2) | 2.84 (2.14–46.96) |     0.094 (0.071–1.556) |
| 32 values          | 78.2 (53.1–78.8) |             23.7 (22.5–25.8) |                     27.9 (27.5–29.0) | 2.50 (1.74–28.77) |     0.082 (0.057–0.955) |
| 10,000 values      | 79.0 (67.3–99.8) |             36.2 (34.9–41.0) |                     44.0 (41.3–44.8) | 2.86 (0.81–14.63) |     0.094 (0.027–0.484) |

Cumulative user and system CPU deltas were converted from Mach ticks using
the host's `mach_timebase_info` scale, 125/3 nanoseconds per tick. An
independent same-process busy loop checked the conversion against
`getrusage(RUSAGE_SELF)`. RSS and physical footprint are separate kernel
counters. [Apple XNU 23.6 task CPU accounting](https://github.com/apple-oss-distributions/xnu/blob/xnu-10063.141.1/osfmk/kern/task.c#L6147-L6153)
[Apple XNU 23.6 rusage counters](https://github.com/apple-oss-distributions/xnu/blob/xnu-10063.141.1/osfmk/kern/bsd_kern.c#L1141-L1166)

Both idle boundary snapshots found zero processes with PPID equal to the
app PID in all 18 completed trials. The measurements include only the
native host process. WebKit's separate service processes, GPU, WindowServer,
and system overhead are outside these figures. The sampled process tree
provides no verified attribution for WebKit helpers. Total desktop
resource usage remains unqualified. These measurements apply to one
host and this isolated build configuration.

## Linux installed-app group measurement

The Linux runner uses the stock installed unsigned debug Debian package, private Xvfb/D-Bus sessions, and fresh app settings/cache/data directories. It repeats the empty, 32-value and 10,000-value cases five times each, interleaved. The generated bank bytes match the earlier native fixtures (1,557 and 488,953 bytes); inputs must remain unchanged. System caches are not flushed. Binary/helper hashes, package versions, kernel, architecture and actual WebDriver capabilities accompany each report.

`linux-cgroup.py` creates one exclusive `starbank-perf-<random token>` leaf under the existing cgroup v2 root. Setup requires memory accounting already enabled and empty zero counters. It never enables ancestor controllers or changes their permissions. The CI-only launcher is installed non-set-id in an exclusive root-owned directory. Through the runner's existing sudo access it verifies the fixed installed binary and leaf identity, moves only itself, closes privileged descriptors, rebuilds the caller's environment/groups and verifies the complete credential drop before executing `/usr/bin/starbank-desktop`. The application must run as the ordinary runner. No arbitrary process ID or executable is accepted.

The measurement scope includes work after group admission, including the launcher's final credential checks and exec. Descendants inherit the group. Shared services outside it, WebDriver, Node and Xvfb are excluded. CPU is the kernel group's cumulative microseconds; current and peak memory are charged bytes and the kernel watermark, **not RSS or a sum of process high-water marks**. Cache pages charged elsewhere remain outside these counters. Concurrent current/peak observations are preserved without normalization. [Kernel accounting and containment](https://docs.kernel.org/admin-guide/cgroup-v2.html)

A separately bounded membership query reads PIDs only from the identified leaf, verifies their process start times and exact cgroup membership, and records executable paths/effective UIDs. Schema-v2 reports also retain exact user/system CPU ticks from each process's first guarded stat read. A second read checks identity and monotonic counters. Each window's endpoint observations must include the installed app with the runner's UID and an actual WebKit Web process. These bracketed metadata reads do not freeze process execution or describe every short-lived process; resource counters retain the full kernel group scope.

Startup timing begins before the WebDriver session POST and includes automation. Empty readiness means the enabled Open Bank action; file readiness includes actual queued Open and the visible PerfProbe Table. Schema v2 measures two windows beginning no sooner than one and fifteen seconds after that same readiness observation. Each contains seven counter reads separated by approximately 500 ms; no WebDriver requests occur after readiness until both windows finish. Raw command timing bounds accompany each sample; group CPU uses first/last command midpoints and reports a percentage of one core. Early work can be deferred startup activity. A later window alone does not establish eventual steady state. No performance threshold or relative speed claim is inferred.

Per-process attribution uses the host's `getconf CLK_TCK` and matches PID, start time, executable and effective UID across each window. Tick differences use exact integer arithmetic before conversion. Membership-request midpoints define a separate duration that brackets the group samples; process percentages are not an exact partition of the group-window percentage. Reports explicitly retain unmatched endpoint identities and membership-query failures. Short-lived processes can contribute to group CPU without appearing in process deltas. Missing required app/WebKit membership fails the trial after preserving both windows. No application behavior changes accompany this diagnostic extension.

The app closes normally and the leaf must become empty before successful session teardown. Cleanup verifies the original device/inode and empty membership before removing only that leaf, never killing processes. All trials, failures and not-run cases are retained. Complete JSON snapshots are published atomically before and after each trial and after each complete sampling window, so an interruption preserves the last valid report. CI retains the reports as artifacts, including failed attempts, and removes its root-owned helper directory and installed test package.

Twenty-four simulated-filesystem tests passed locally and in hosted source checks, including exact large process ticks, exclusion of waited-for child ticks, malformed tick fields and monotonicity. They also cover counter fields, process identity/membership changes, existing leaves, controller gaps, malformed data and refusal to remove populated groups. Five pure Node attribution tests cover exact tick differences, separate midpoint spans, identity changes, missing snapshots and invalid deltas. These passed locally and on both hosted Linux architectures. Strict C11 object compilation previously passed for Linux x86_64 and arm64 against glibc 2.39 headers; the launcher is unchanged. Node syntax, Biome and frontend typechecks passed. Real admission and schema-v1 measurements passed at `2806e52`; schema-v2 measurements passed at `6c68a33`, as recorded below. Simulated tests and static checks remain separate evidence. [Runner](linux-performance.mjs) · [Launcher](linux-app-launcher.c) · [Simulated filesystem tests](test_linux_cgroup.py) · [Attribution tests](linux-performance.test.mjs)

## Hosted Linux baseline at `2806e52`

All eleven jobs passed in run `46108490128`. Each architecture completed all 15 real trials, five per scenario, with seven populated counter samples each. All 60 readiness/end membership snapshots contained exactly the installed app, WebKit Web process and WebKit Network process under effective UID 1001. Every input-byte check, empty-before-session-delete check, leaf removal, helper-directory removal and package purge passed. The source job separately passed 22 simulated filesystem tests. Native accessibility and all three functional sessions also passed. [Hosted run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46108490128)

The hosted environment used kernel `7.0.0-1020-azure`, four reported CPUs, Node 24.22.3, WebKitGTK `2.52.6-0ubuntu0.24.04.2` and Wry 0.57.2. Reports retain the exact installed binary/helper digests and package versions. These are unsigned debug builds with unflushed system caches, not release-performance or game-workload results.

Medians below are across the five trials for each scenario. Memory is the kernel charged-memory watermark as of the early sampling window's end, in MiB (2^20 bytes). It includes startup and the admission launcher's final work, excludes services outside the group, and is not RSS. The CPU window begins approximately one second after readiness and lasts roughly 3.34 seconds; it is an **early no-input window**, not established steady-state idle utilization.

| Architecture | Values | POST to enabled action, ms | Open click to Table, ms | Charged peak, MiB | Early CPU, % one core |
| ------------ | -----: | -------------------------: | ----------------------: | ----------------: | --------------------: |
| x64          |      0 |                    1,089.5 |                       — |           211.254 |                 0.047 |
| x64          |     32 |                    1,200.9 |                   172.2 |           241.102 |                52.987 |
| x64          | 10,000 |                    1,187.0 |                   524.3 |           283.578 |                50.911 |
| arm64        |      0 |                    1,070.0 |                       — |           221.012 |                 0.060 |
| arm64        |     32 |                    1,158.2 |                   169.3 |           253.867 |                80.604 |
| arm64        | 10,000 |                    1,160.5 |                   645.8 |           300.766 |                77.299 |

Loaded-bank CPU began near zero, rose in the second sampling interval and stayed elevated through the final four intervals on both architectures. That observation does not locate the work or prove eventual steady-state behavior. The fixed empty/small/large trial order also confounds scenario and order. No app change or performance budget is justified by attribution guesses; a later window and per-process CPU deltas are the next measurements.

The complete schema-v1 reports are retained as [x64 data](linux-x64-2806e52.json) and [arm64 data](linux-arm64-2806e52.json). Repository copies are whitespace-formatted; parsed JSON was checked equal to the downloaded originals. Original JSON SHA-256 values were `7dc904916e2d5944efe207cd919d0f8937b2149a36d58dfa0a6bdc9bddd78152` and `d0eac12a238b95cd41d8ea4f058d9f5304ae33c257eb93bd3bfd400ad2975bb6`. Current-version report fields must not be retroactively added to these observations. [Original x64 artifact](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46108490128/artifacts/15248787071) · [Original arm64 artifact](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46108490128/artifacts/15248219505)

## Early and later Linux windows at `6c68a33`

All eleven jobs passed in run `46110611436`. Both architectures completed 15 trials, five per scenario, with seven populated samples in each early and later window. All 120 membership snapshots contained exactly the installed app, WebKit Web process and WebKit Network process under effective UID 1001. Every attribution matched endpoint identities without unmatched processes. All input-byte checks, empty-before-session-delete checks, leaf removals, helper removals and package purges passed. Both native accessibility controls and the three functional sessions passed; each architecture also passed 114 desktop tests and all ten Node helper tests. [Hosted run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46110611436)

Both runners used kernel `7.0.0-1020-azure`, four reported CPUs, Node 24.22.3, WebKitGTK `2.52.6-0ubuntu0.24.04.2` and `CLK_TCK=100`. The same unsigned debug, automation-inclusive, unflushed-cache and synthetic-input limitations apply. CPU percentages below use one core as the denominator. Each early/later entry is the median across five trials; the later range shows all five observations.

| Architecture | Values | Early group CPU, % | Later group CPU, % | Later range, % | Early Web process CPU, % | Early app CPU, % |
| ------------ | -----: | -----------------: | -----------------: | -------------: | -----------------------: | ---------------: |
| x64          |      0 |              0.059 |              0.042 |    0.033–0.073 |                    0.000 |            0.000 |
| x64          |     32 |             47.061 |              0.032 |    0.027–0.036 |                   44.418 |            2.043 |
| x64          | 10,000 |             44.961 |              0.014 |    0.014–0.015 |                   42.565 |            2.046 |
| arm64        |      0 |              0.057 |              0.040 |    0.035–0.043 |                    0.000 |            0.000 |
| arm64        |     32 |             78.829 |              0.037 |    0.032–0.050 |                   75.415 |            2.901 |
| arm64        | 10,000 |             75.089 |              0.045 |    0.027–0.051 |                   71.258 |            3.190 |

Most matched CPU time in the early loaded-bank windows belonged to the WebKit Web process. The rise had subsided in the later windows starting at least fifteen seconds after readiness. All later group observations were below 0.074% of one core. Later per-process medians were zero ticks; individual nonzero deltas were one tick (10 ms) in the app or Web process. Zero ticks does not mean zero work. Process attribution uses a separate bracketing midpoint span and clock-tick resolution, so it must not be subtracted from the group percentage. These finite observations locate early process activity but neither identify the underlying task nor prove eventual steady-state behavior. No application code was changed on the basis of the early rise.

| Architecture | Values | POST to enabled action, ms | Open click to Table, ms | Charged peak at later window, MiB |
| ------------ | -----: | -------------------------: | ----------------------: | --------------------------------: |
| x64          |      0 |                    1,029.4 |                       — |                           210.473 |
| x64          |     32 |                    1,132.9 |                   167.2 |                           244.352 |
| x64          | 10,000 |                    1,125.5 |                   497.1 |                           284.594 |
| arm64        |      0 |                    1,044.9 |                       — |                           220.801 |
| arm64        |     32 |                    1,133.2 |                   165.7 |                           255.812 |
| arm64        | 10,000 |                    1,159.2 |                   633.5 |                           299.289 |

Charged-memory peaks include startup and post-admission launcher work, exclude services outside the leaf and are not RSS. Medians across different hosted machines and runs are observations, not controlled speed or memory comparisons. Representative real workloads, release builds, other operating systems' helper accounting, longer-term behavior and owner-approved budgets remain open.

Complete schema-v2 [x64 data](linux-x64-6c68a33.json) and [arm64 data](linux-arm64-6c68a33.json) are preserved with whitespace formatting only; parsed JSON equality was checked against the downloaded originals. Original JSON SHA-256 values were `d46d93446fd3474d7005e7b6500c4f7545e50e9d07e852ddeb2a0b879ad1dd3e` and `48cdc228836cac9b7c0c576c03681c1f6714535be33c2d9e734e296d30bf8c87`. [Original x64 artifact](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46110611436/artifacts/15249443910) · [Original arm64 artifact](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46110611436/artifacts/15249093759)

The earlier schema-v2 run at `83f8ac2` also completed all 15 trials on both architectures, with later group CPU ranges of 0.014–0.097% on x64 and 0.028–0.049% on arm64. Both Linux jobs passed; the overall run finished ten of eleven because the web job rejected Markdown table formatting. The formatting fix and durable attribution tests preceded the fully green run above. The earlier x64 host used kernel `7.0.0-1018-aws`, so this is an additional scoped observation, not a paired performance comparison. Original JSON SHA-256 values were `4f38c51e72f6859dc992863bacf98eed4bad2dad823ceaa926a568f531e27db7` and `ba214e991c8a7a81fdd47b792cd59309509537d9667b0db67bea42d6f87ddee3`. [Earlier run](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46110448684) · [x64 artifact](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46110448684/artifacts/15249287801) · [arm64 artifact](https://github.com/PixelSoftwareStudios/StarBank-Reloaded/actions/runs/46110448684/artifacts/15248983863)

## Reproduction

The exact measured revision no longer resolves in the local checkout after the history rewrite. The archive command identifies the original input. Record a new run and its actual revision for a current measurement under the applicable testing policy. [Historical evidence](../../../../docs/verification/VERIFICATION_REGISTER.md#historical-evidence)

Keep `prepare-scratch.py`, `native-perf.swift`, `cpu-units.swift`, and
`summarize.py` together in a fresh scratch directory. Run from that directory
with `STARBANK_REPOSITORY` set to the StarBank checkout. Use Node 24.22.3,
pnpm 12.23.0, Swift 6.0.3 and the repository Rust toolchain, with dependencies
already available locally or fetched through the ordinary pinned build
workflow. Run the Swift launcher from an interactive macOS desktop session
with permission to read the launched application's accessibility tree.

```sh
mkdir source
git -C "$STARBANK_REPOSITORY" archive e2cf03533264146e23815f38f1353c2d5c829555 | tar -x -C source
python3 prepare-scratch.py
(
  cd source
  pnpm install --frozen-lockfile
  pnpm --filter starbank-desktop build:webview
  cargo build --release --locked -p starbank-desktop --features custom-protocol
)
scratch="$(pwd -P)"
swiftc -module-cache-path "$scratch/swift-cache" -O native-perf.swift -o native-perf
swift -module-cache-path "$scratch/swift-cache" cpu-units.swift
./native-perf "$scratch/source/target/release/starbank-desktop" "$scratch/measurement" 5 > samples.jsonl
python3 summarize.py samples.jsonl 125/3
```

Use the timebase ratio reported by `cpu-units.swift` on the measurement host.
The tracked `samples.jsonl` contains the measured data. The original
capture used a `_ns` suffix on the CPU counter fields. The checked-in data
and runner use `_ticks`. The underlying counter values are unchanged.

## Linux release measurement gate

The [release workflow](../../../../.github/workflows/linux-release-performance.yml) adds separate Ubuntu x64 and arm64 jobs for an unsigned optimized desktop package. Its first hosted execution is pending; the existing measurements below retain their original profile and environment labels.

The build runs `pnpm --filter starbank-desktop exec tauri build --bundles deb --no-sign -- --locked --message-format=json-render-diagnostics`. The [receipt checker](linux-release-build.mjs) requires exactly one desktop binary compiler artifact with optimization level 3, debug assertions and test mode off, and desktop/custom-protocol features. The receipt records the exact read-only DEB and extracted packaged executable hashes. Tauri may patch the packaged executable's bundle marker, so compiler output bytes are not assumed to equal packaged bytes.

After payload verification and minimal-Ubuntu dependency qualification, installation must match the receipt's DEB hash. The measurement observer requires the installed executable to match the packaged name, size and SHA-256 before any trial. It then reuses the existing fifteen cgroup trials, readiness checks, early and later windows, byte-preservation checks and owned cleanup. The extended invocation is `node linux-performance.mjs <root-owned-tools> <close-sender> <report.json> release <readonly-deb> <build-receipt.json>`; the original three-argument debug invocation remains supported.

The workflow retains the mixed Cargo build log, compiler/package receipt and incremental measurement report. Only completed trials with matching executable admission and confirmed cleanup qualify as results. A hard runner timeout can interrupt cleanup and does not establish success. These synthetic, automation-inclusive, unflushed-cache observations will not establish real-workload budgets or a controlled comparison with separately hosted debug runs. Twenty-eight local Node helper tests passed before integration; actual release builds and measurements still require hosted execution.
