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

A separately bounded membership query reads PIDs only from the identified leaf, verifies their process start times and exact cgroup membership, and records executable paths/effective UIDs. Both readiness and idle-end observations must include the installed app with the runner's UID and an actual WebKit Web process. These bracketed metadata reads do not freeze process execution or describe every short-lived process; resource counters retain the full kernel group scope.

Startup timing begins before the WebDriver session POST and includes automation. Empty readiness means the enabled Open Bank action; file readiness includes actual queued Open and the visible PerfProbe Table. After a one-second settle, seven counter reads are separated by approximately 500 ms with no WebDriver requests during sampling. Raw command timing bounds accompany each sample; idle CPU uses first/last command midpoints and reports a percentage of one core. No performance threshold or relative speed claim is inferred.

The app closes normally and the leaf must become empty before successful session teardown. Cleanup verifies the original device/inode and empty membership before removing only that leaf, never killing processes. All trials, failures and not-run cases are retained. Complete JSON snapshots are published atomically before and after each trial, so an interruption preserves the last valid report. CI retains the reports as artifacts, including failed attempts, and removes its root-owned helper directory and installed test package.

Twenty-two simulated-filesystem tests passed locally. They cover counter fields, process identity/membership changes, existing leaves, controller gaps, malformed data and refusal to remove populated groups. Strict C11 object compilation passed for Linux x86_64 and arm64 against glibc 2.39 headers; Node syntax, Biome and import-without-launch checks passed. **Real app admission and hosted measurements are pending.** These tests and static checks are not measured resource results. [Runner](linux-performance.mjs) · [Launcher](linux-app-launcher.c) · [Simulated tests](test_linux_cgroup.py)

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
