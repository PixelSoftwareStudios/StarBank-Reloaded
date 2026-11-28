import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  link,
  lstat,
  mkdtempDisposable,
  rename,
  writeFile,
} from "node:fs/promises";
import { availableParallelism, release, tmpdir, totalmem } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";
import { requestGtkWindowClose } from "../linux-gtk-dialog.mjs";
import { withNativeSession } from "../linux-native-webview.mjs";

const exec = promisify(execFile);
const sudo = "/usr/bin/sudo";
const python = "/usr/bin/python3";
const installedApp = "/usr/bin/starbank-desktop";
const trustedDirectory = /^\/usr\/local\/lib\/starbank-perf-[a-f0-9]{32}$/;
const bankName = "Synthetic.SC2Bank";
const sectionName = "PerfProbe";
const appTitle = "StarBank Reloaded";
const webKitProcess = "WebKitWebProcess";
const fileQueue = 'aside[aria-label="File open requests"]';
const webdriverElement = "element-6066-11e4-a52e-4f735466cecf";
const repetitions = 5;
const earlySettleMs = 1_000;
const lateSettleMs = 15_000;
const sampleCount = 7;
const sampleIntervalMs = 500;
const readinessTimeoutMs = 10_000;
const closureTimeoutMs = 10_000;
const commandTimeoutMs = 4_000;
const readyPollMs = 100;
const outputLimit = 128 * 1024;
const maxMembers = 256;
const maxDiagnosticChars = 1_000;
const maxU64 = (1n << 64n) - 1n;
const maxSafeInteger = BigInt(Number.MAX_SAFE_INTEGER);
const launcherEnvironment = [
  "WEBKIT_INSPECTOR_SERVER",
  "GTK_OVERLAY_SCROLLING",
  "DISPLAY",
  "DBUS_SESSION_BUS_ADDRESS",
  "TAURI_WEBVIEW_AUTOMATION",
  "GTK_USE_PORTAL",
  "G_DEBUG",
  "LC_ALL",
  "XAUTHORITY",
  "XDG_CONFIG_HOME",
  "XDG_CACHE_HOME",
  "XDG_DATA_HOME",
  "XDG_RUNTIME_DIR",
];

function syntheticBank(values) {
  const keys = Array.from({ length: values }, (_, index) => {
    const name = `Value${String(index).padStart(5, "0")}`;
    return `<Key name="${name}"><Value int="${index}"/></Key>`;
  }).join("\n");
  return Buffer.from(
    `<Bank version="1"><Section name="${sectionName}">\n${keys}\n</Section></Bank>\n`,
  );
}

function makeScenarios() {
  return [
    { name: "empty", values: 0, inputs: [] },
    { name: "small", values: 32, inputs: [[bankName, syntheticBank(32)]] },
    {
      name: "large",
      values: 10_000,
      inputs: [[bankName, syntheticBank(10_000)]],
    },
  ];
}

async function invoke(file, args, timeout = commandTimeoutMs) {
  const { stdout } = await exec(file, args, {
    encoding: "utf8",
    killSignal: "SIGKILL",
    maxBuffer: outputLimit,
    timeout,
  });
  return stdout;
}

async function checkedPath(path, type) {
  const info = await lstat(path);
  const expectedType =
    type === "directory" ? info.isDirectory() : info.isFile();
  assert(
    expectedType && info.uid === 0 && (info.mode & 0o022) === 0,
    `Expected a root-owned, non-writable ${type} at ${path}.`,
  );
  return info;
}

async function installedHelpers(directory) {
  assert(
    trustedDirectory.test(directory),
    "Expected the root-owned libexec directory.",
  );
  await checkedPath(dirname(directory), "directory");
  await checkedPath(directory, "directory");
  const launcher = join(directory, "linux-app-launcher");
  const cgroup = join(directory, "linux-cgroup.py");
  const info = await checkedPath(launcher, "executable");
  assert(
    (info.mode & 0o6000) === 0 && (info.mode & 0o001) !== 0,
    "The launcher must be executable and must not be set-id.",
  );
  await checkedPath(cgroup, "helper script");
  return { launcher, cgroup };
}

function canonicalUnsigned(value) {
  return (
    typeof value === "string" &&
    /^(0|[1-9][0-9]{0,19})$/.test(value) &&
    BigInt(value) <= maxU64
  );
}

function leafHandle(value) {
  assert(
    value && Object.keys(value).sort().join(",") === "device,inode,token",
    "Invalid cgroup handle fields.",
  );
  assert(
    typeof value.token === "string" &&
      /^[a-f0-9]{32}$/.test(value.token) &&
      canonicalUnsigned(value.device) &&
      canonicalUnsigned(value.inode) &&
      value.inode !== "0",
    "Invalid cgroup handle identity.",
  );
  return value;
}

function counters(value) {
  assert(
    value &&
      Object.keys(value).sort().join(",") ===
        "cpu_usage_usec,memory_current_bytes,memory_peak_bytes,populated",
    "Invalid cgroup counter fields.",
  );
  for (const field of [
    "cpu_usage_usec",
    "memory_current_bytes",
    "memory_peak_bytes",
  ])
    assert(
      Number.isSafeInteger(value[field]) && value[field] >= 0,
      `Invalid ${field} counter.`,
    );
  assert(
    typeof value.populated === "boolean",
    "Invalid cgroup population flag.",
  );
  return value;
}

function cgroupCommands(helper) {
  const privileged = (command, ...args) =>
    invoke(sudo, ["-n", python, "-I", helper, command, ...args]);
  return {
    async setup() {
      return JSON.parse(await privileged("setup"));
    },
    async sample(leaf, timeout) {
      return counters(
        JSON.parse(
          await invoke(
            python,
            ["-I", helper, "sample", JSON.stringify(leaf)],
            timeout,
          ),
        ),
      );
    },
    async members(leaf) {
      const { members } = JSON.parse(
        await privileged("members", JSON.stringify(leaf)),
      );
      assert(
        Array.isArray(members) && members.length <= maxMembers,
        "Invalid cgroup membership list.",
      );
      const pids = new Set();
      for (const member of members) {
        assert(
          member &&
            Object.keys(member).sort().join(",") ===
              "effective_uid,executable,pid,starttime_ticks,system_cpu_ticks,user_cpu_ticks",
          "Invalid cgroup member fields.",
        );
        assert(
          canonicalUnsigned(member.pid) &&
            member.pid !== "0" &&
            canonicalUnsigned(member.starttime_ticks) &&
            canonicalUnsigned(member.effective_uid) &&
            canonicalUnsigned(member.user_cpu_ticks) &&
            canonicalUnsigned(member.system_cpu_ticks) &&
            typeof member.executable === "string" &&
            isAbsolute(member.executable),
          "Invalid cgroup member identity.",
        );
        assert(!pids.has(member.pid), "Duplicate cgroup member PID.");
        pids.add(member.pid);
      }
      return members;
    },
    async cleanup(leaf) {
      assert.equal(await privileged("cleanup", JSON.stringify(leaf)), "");
    },
  };
}

async function pollUntil(description, read, timeout = readinessTimeoutMs) {
  const deadline = performance.now() + timeout;
  for (;;) {
    const remaining = Math.floor(deadline - performance.now());
    if (remaining <= 0)
      throw new Error(`Timed out waiting for ${description}.`);
    const result = await read(remaining);
    if (result && performance.now() < deadline) return result;
    const sleep = Math.floor(
      Math.min(readyPollMs, deadline - performance.now()),
    );
    if (sleep > 0) await delay(sleep);
  }
}

function evaluate(command, script, ...args) {
  return command("/execute/sync", "POST", { script, args });
}

function readyAction(command, file) {
  return evaluate(
    command,
    `
    const [selector, bank, title] = arguments;
    const queue = document.querySelector(selector);
    const marker = bank
      ? Array.from(queue?.querySelectorAll("p[title]") ?? []).some((node) => node.title === bank && node.textContent.trim() === bank)
      : document.querySelector(".editor-welcome h1")?.textContent.trim() === title;
    if (!marker) return null;
    return Array.from((bank ? queue : document).querySelectorAll("button")).find((button) =>
      button.textContent.trim() === (bank ? "Open" : "Open Bank…") &&
      button.getClientRects().length > 0 && !button.matches(":disabled") &&
      button.getAttribute("aria-disabled") !== "true" && !button.hasAttribute("data-disabled")) ?? null;
  `,
    fileQueue,
    file ? bankName : null,
    appTitle,
  );
}

function renderedTable(command) {
  return evaluate(
    command,
    `
    const [bank, section] = arguments;
    const visible = (element) => element && element.getClientRects().length > 0;
    const heading = document.querySelector(".document-editor h1");
    const tab = document.querySelector('button[role="tab"][aria-selected="true"]');
    const sectionButton = Array.from(document.querySelectorAll('nav[aria-label="Bank sections"] button'))
      .find((button) => button.textContent.trim() === section);
    const first = document.querySelector('table input[aria-label="Value00000 value"]');
    return Boolean(visible(heading) && heading.textContent.trim() === bank &&
      visible(tab) && tab.textContent.trim() === "Table View" && visible(sectionButton) &&
      visible(first) && first.value === "0");
  `,
    bankName,
    sectionName,
  );
}

async function observeReadiness(command, context, trial, file) {
  const { sessionStartedAt, sessionCreatedAt, capabilities } = context;
  assert(
    Number.isFinite(sessionStartedAt) &&
      Number.isFinite(sessionCreatedAt) &&
      sessionCreatedAt >= sessionStartedAt,
    "Missing monotonic session timing.",
  );
  trial.timing = {
    session_post_to_created_ms: sessionCreatedAt - sessionStartedAt,
  };
  const { browserName, browserVersion } = capabilities;
  assert(
    typeof browserName === "string" &&
      browserName &&
      typeof browserVersion === "string" &&
      browserVersion,
    "Missing the actual WebDriver browser name or version.",
  );
  trial.webdriver_browser = { name: browserName, version: browserVersion };
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  const action = await pollUntil("the enabled packaged Open action", () =>
    readyAction(command, file),
  );
  const actionReadyAt = performance.now();
  trial.timing.session_post_to_enabled_action_ms =
    actionReadyAt - sessionStartedAt;
  if (file) {
    assert.equal(
      typeof action[webdriverElement],
      "string",
      "Missing queued Open element ID.",
    );
    const clickedAt = performance.now();
    await command(`/element/${action[webdriverElement]}/click`, "POST", {});
    await pollUntil("the visible PerfProbe Table", () =>
      renderedTable(command),
    );
    const tableReadyAt = performance.now();
    trial.timing.open_click_to_visible_table_ms = tableReadyAt - clickedAt;
    trial.timing.session_post_to_visible_table_ms =
      tableReadyAt - sessionStartedAt;
    return tableReadyAt;
  }
  return actionReadyAt;
}

function assertMeasuredMembers(members, uid) {
  const application = members.filter(
    (member) => member.executable === installedApp,
  );
  assert.equal(
    application.length,
    1,
    "Expected one installed app in the measurement leaf.",
  );
  assert.equal(
    application[0].effective_uid,
    uid,
    "The measured app must run as the non-root runner.",
  );
  assert(
    members.some(
      (member) =>
        basename(member.executable) === webKitProcess &&
        member.effective_uid === uid,
    ),
    "The measurement leaf did not include the app's WebKit Web process.",
  );
  return application[0];
}

function memberIdentity({ pid, starttime_ticks, executable, effective_uid }) {
  return { pid, starttime_ticks, executable, effective_uid };
}

function sameMember(start, end) {
  return (
    start.pid === end.pid &&
    start.starttime_ticks === end.starttime_ticks &&
    start.executable === end.executable &&
    start.effective_uid === end.effective_uid
  );
}

async function captureMembers(cgroup, leaf, readyAt) {
  const request_started_ms_after_readiness = performance.now() - readyAt;
  let result;
  try {
    result = { members: await cgroup.members(leaf) };
  } catch (error) {
    result = { failure: describeError(error) };
  }
  return {
    request_started_ms_after_readiness,
    request_finished_ms_after_readiness: performance.now() - readyAt,
    ...result,
  };
}

function cpuTickDelta(start, end) {
  const delta = BigInt(end) - BigInt(start);
  assert(delta >= 0n, "A matched process CPU tick counter moved backwards.");
  return delta;
}

function attributeCpu(start, end, ticksPerSecond) {
  if (!start.members || !end.members) return { status: "unavailable" };
  const startByPid = new Map(
    start.members.map((member) => [member.pid, member]),
  );
  const endByPid = new Map(end.members.map((member) => [member.pid, member]));
  const matched = [];
  const startOnly = [];
  const endOnly = [];
  const midpoint = (request) =>
    (request.request_started_ms_after_readiness +
      request.request_finished_ms_after_readiness) /
    2;
  const duration = midpoint(end) - midpoint(start);
  assert(duration > 0, "The membership sample window moved backwards.");
  for (const member of start.members) {
    const ending = endByPid.get(member.pid);
    if (!ending || !sameMember(member, ending)) {
      startOnly.push(memberIdentity(member));
      continue;
    }
    const user = cpuTickDelta(member.user_cpu_ticks, ending.user_cpu_ticks);
    const system = cpuTickDelta(
      member.system_cpu_ticks,
      ending.system_cpu_ticks,
    );
    const total = user + system;
    assert(
      total <= maxSafeInteger,
      "A process CPU tick delta is out of range.",
    );
    const cpuMs = (Number(total) / ticksPerSecond) * 1_000;
    const corePercent = (cpuMs / duration) * 100;
    assert(
      cpuMs <= Number.MAX_SAFE_INTEGER && Number.isFinite(corePercent),
      "The process CPU delta cannot be safely represented.",
    );
    matched.push({
      ...memberIdentity(member),
      user_cpu_delta_ticks: Number(user),
      system_cpu_delta_ticks: Number(system),
      cpu_delta_ms: cpuMs,
      cpu_percent_of_one_core: corePercent,
    });
  }
  for (const member of end.members) {
    const starting = startByPid.get(member.pid);
    if (!starting || !sameMember(starting, member))
      endOnly.push(memberIdentity(member));
  }
  return {
    status:
      startOnly.length || endOnly.length ? "partial" : "matched_endpoints",
    membership_midpoint_span_ms: duration,
    matched,
    start_only: startOnly,
    end_only: endOnly,
  };
}

async function measureIdle(
  cgroup,
  leaf,
  window,
  readyAt,
  settle,
  ticksPerSecond,
) {
  const earliest = readyAt + settle;
  while (performance.now() < earliest) {
    const remaining = Math.ceil(earliest - performance.now());
    if (remaining > 0) await delay(remaining);
  }
  window.members_at_start = await captureMembers(cgroup, leaf, readyAt);
  window.samples = [];
  const started = performance.now();
  window.samples_started_ms_after_readiness = started - readyAt;
  for (let index = 0; index < sampleCount; index++) {
    if (index > 0) await delay(sampleIntervalMs);
    const before = performance.now();
    const sample = await cgroup.sample(leaf);
    const after = performance.now();
    window.samples.push({
      request_midpoint_ms: (before + after) / 2 - started,
      request_duration_ms: after - before,
      ...sample,
    });
    assert(
      sample.populated,
      "The app left the measurement leaf during the sample window.",
    );
  }
  window.members_at_end = await captureMembers(cgroup, leaf, readyAt);
  const first = window.samples[0];
  const last = window.samples.at(-1);
  const duration = last.request_midpoint_ms - first.request_midpoint_ms;
  const cpu = last.cpu_usage_usec - first.cpu_usage_usec;
  assert(
    duration > 0 && cpu >= 0,
    "The sample window or CPU counter moved backwards.",
  );
  window.idle = {
    sample_midpoint_span_ms: duration,
    cpu_usage_delta_usec: cpu,
    cpu_percent_of_one_core: (cpu / (duration * 1_000)) * 100,
    final_memory_peak_bytes: last.memory_peak_bytes,
  };
  window.attribution = attributeCpu(
    window.members_at_start,
    window.members_at_end,
    ticksPerSecond,
  );
}

function verifyTrialMembership(trial, uid) {
  const snapshots = [
    trial.members_at_start,
    trial.members_at_end,
    trial.late.members_at_start,
    trial.late.members_at_end,
  ];
  let app;
  for (const snapshot of snapshots) {
    assert(snapshot.members, "A membership snapshot could not be captured.");
    const observedApp = assertMeasuredMembers(snapshot.members, uid);
    assert(!app || sameMember(app, observedApp), "The measured app changed.");
    app = observedApp;
  }
}

function describeError(error, depth = 0) {
  const message = error instanceof Error ? error.message : String(error);
  const description = { message: message.slice(0, maxDiagnosticChars) };
  if (typeof error?.stderr === "string" && error.stderr)
    description.stderr = error.stderr.slice(0, maxDiagnosticChars);
  if (depth < 4) {
    if (error instanceof AggregateError)
      description.errors = error.errors.map((child) =>
        describeError(child, depth + 1),
      );
    if (error instanceof Error && error.cause)
      description.cause = describeError(error.cause, depth + 1);
  }
  return description;
}

function recordFailure(trial, stage, error) {
  trial.failures.push({ stage, ...describeError(error) });
}

function waitForLeafEmpty(cgroup, leaf) {
  return pollUntil(
    "an empty owned measurement leaf",
    async (remaining) =>
      !(await cgroup.sample(leaf, Math.min(remaining, commandTimeoutMs)))
        .populated,
    closureTimeoutMs,
  );
}

async function observeTrial(command, context, fixture) {
  const {
    cgroup,
    leaf,
    scenario,
    trial,
    uid,
    closeSender,
    ticksPerSecond,
    checkpoint,
  } = fixture;
  const errors = [];
  try {
    const readyAt = await observeReadiness(
      command,
      context,
      trial,
      scenario.inputs.length > 0,
    );
    await measureIdle(
      cgroup,
      leaf,
      trial,
      readyAt,
      earlySettleMs,
      ticksPerSecond,
    );
    await checkpoint();
    trial.late = {};
    await measureIdle(
      cgroup,
      leaf,
      trial.late,
      readyAt,
      lateSettleMs,
      ticksPerSecond,
    );
    await checkpoint();
    verifyTrialMembership(trial, uid);
  } catch (error) {
    recordFailure(trial, "observe", error);
    errors.push(error);
  }
  try {
    await context.assertInputsUnchanged();
    trial.source_bytes_unchanged = true;
  } catch (error) {
    recordFailure(trial, "input bytes", error);
    errors.push(error);
  }
  try {
    await requestGtkWindowClose(closeSender);
    await waitForLeafEmpty(cgroup, leaf);
    trial.empty_before_session_delete = true;
  } catch (error) {
    recordFailure(trial, "natural window close", error);
    errors.push(error);
  }
  if (errors.length)
    throw new AggregateError(
      errors,
      "The measurement session did not qualify.",
    );
}

async function runTrial(scenario, trial, shim, observer) {
  const { cgroup } = observer;
  trial.failures = [];
  trial.empty_before_session_delete = false;
  let leaf;
  try {
    const setup = await cgroup.setup();
    leaf = leafHandle(setup.leaf);
    trial.leaf = leaf;
    trial.baseline = counters(setup.baseline);
    assert.deepEqual(trial.baseline, {
      cpu_usage_usec: 0,
      memory_current_bytes: 0,
      memory_peak_bytes: 0,
      populated: false,
    });
    await withNativeSession(
      shim,
      `performance ${scenario.name} ${trial.iteration}`,
      scenario.inputs,
      (command, context) =>
        observeTrial(command, context, {
          ...observer,
          leaf,
          scenario,
          trial,
        }),
      { applicationArgumentsPrefix: [leaf.token, leaf.device, leaf.inode] },
    );
  } catch (error) {
    recordFailure(trial, leaf ? "session" : "leaf setup", error);
  }
  if (leaf) {
    try {
      await waitForLeafEmpty(cgroup, leaf);
      await cgroup.cleanup(leaf);
      trial.leaf_removed = true;
    } catch (error) {
      trial.leaf_removed = false;
      recordFailure(trial, "leaf cleanup", error);
    }
  }
  trial.status = trial.failures.length === 0 ? "passed" : "failed";
  console.log(
    `Linux native performance ${scenario.name} ${trial.iteration}: ${trial.status}.`,
  );
  return trial.leaf_removed === true;
}

async function sha256(path) {
  const digest = createHash("sha256");
  for await (const chunk of createReadStream(path)) digest.update(chunk);
  return digest.digest("hex");
}

function snapshotWriter(output, scratch) {
  let previous;
  let sequence = 0;
  return async (report) => {
    const snapshot = join(scratch, `report-${++sequence}.json`);
    await writeFile(snapshot, `${JSON.stringify(report, null, 2)}\n`, {
      flag: "wx",
      mode: 0o600,
      flush: true,
    });
    const next = await lstat(snapshot, { bigint: true });
    if (previous) {
      const current = await lstat(output, { bigint: true });
      assert(
        current.isFile() &&
          current.dev === previous.dev &&
          current.ino === previous.ino,
        "The performance output no longer names the previous snapshot.",
      );
      await rename(snapshot, output);
    } else {
      // A hard link publishes the initial complete snapshot without overwriting
      // any existing file or leaving an empty output during initialization.
      await link(snapshot, output);
    }
    previous = next;
  };
}

async function measure(helperDirectory, closeSender, output) {
  const scenarios = makeScenarios();
  const report = {
    schema_version: 2,
    started_at: new Date().toISOString(),
    build_scope:
      "Installed unsigned Linux debug Debian package on a hosted Xvfb runner",
    accounting_scope:
      "Fresh cgroup v2 leaf containing the app and its descendants, including the final work of the admission launcher. Shared services outside the leaf are excluded. Memory counters are cgroup-charged bytes and kernel peak watermarks.",
    timing_scope:
      "Fresh app and WebView processes launch with unflushed system caches. Startup includes automation from WebDriver session POST through observed UI readiness. Quiet CPU windows span first and last cgroup sample-command midpoints, with no WebDriver calls after readiness until both windows finish. The early window can include work deferred from startup.",
    attribution_scope:
      "Process CPU deltas use Linux clock ticks for matching PID, start time, executable and effective UID at each window's endpoints. Membership commands bracket the group samples and have a separate midpoint span. Processes missing at either endpoint or present only between endpoints can contribute to group totals without appearing in process deltas.",
    settle_ms: earlySettleMs,
    late_settle_ms: lateSettleMs,
    samples_per_window: sampleCount,
    sample_interval_ms: sampleIntervalMs,
    trials: Array.from({ length: repetitions }, (_, index) =>
      scenarios.map((scenario) => ({
        scenario: scenario.name,
        iteration: index + 1,
        status: "not_run",
      })),
    ).flat(),
  };
  await using snapshots = await mkdtempDisposable(
    join(dirname(output), ".starbank-linux-perf-report-"),
  );
  const persist = snapshotWriter(output, snapshots.path);
  await persist(report);
  try {
    const helpers = await installedHelpers(helperDirectory);
    const app = await checkedPath(installedApp, "application");
    const close = await lstat(closeSender);
    assert(
      close.isFile() && (close.mode & 0o111) !== 0,
      "Expected the native close sender executable.",
    );
    const uid = String(process.getuid());
    const ticks = (await invoke("/usr/bin/getconf", ["CLK_TCK"])).trim();
    assert(canonicalUnsigned(ticks), "Invalid Linux CLK_TCK value.");
    const ticksPerSecond = Number(ticks);
    assert(
      Number.isSafeInteger(ticksPerSecond) && ticksPerSecond > 0,
      "Linux CLK_TCK must be a positive safe integer.",
    );
    const packageRows = await invoke("/usr/bin/dpkg-query", [
      "-W",
      `-f=\${Package}\t\${Version}\t\${Architecture}\n`,
      "starbank-reloaded",
      "libwebkit2gtk-4.1-0",
      "webkit2gtk-driver",
    ]);
    report.environment = {
      revision: process.env.GITHUB_SHA ?? null,
      run_id: process.env.GITHUB_RUN_ID ?? null,
      run_attempt: process.env.GITHUB_RUN_ATTEMPT ?? null,
      kernel: release(),
      architecture: process.arch,
      node: process.version,
      process_clock_ticks_per_second: ticksPerSecond,
      available_cpu_count: availableParallelism(),
      physical_memory_bytes: totalmem(),
      executable_bytes: app.size,
      executable_sha256: await sha256(installedApp),
      launcher_sha256: await sha256(helpers.launcher),
      cgroup_helper_sha256: await sha256(helpers.cgroup),
      installed_packages: packageRows
        .trim()
        .split("\n")
        .map((row) => row.split("\t")),
    };
    report.fixtures = scenarios.map(({ name, values, inputs }) => ({
      scenario: name,
      values,
      file_bytes: inputs[0]?.[1].length ?? 0,
      sha256: inputs.length
        ? createHash("sha256").update(inputs[0][1]).digest("hex")
        : null,
    }));
    await persist(report);
    await using scratch = await mkdtempDisposable(
      join(tmpdir(), "starbank-linux-perf-shim-"),
    );
    const shim = join(scratch.path, "launch-starbank");
    await writeFile(
      shim,
      `#!/bin/sh\nexec ${sudo} -n --preserve-env=${launcherEnvironment.join(",")} ${helpers.launcher} "$@"\n`,
      { flag: "wx", mode: 0o700 },
    );
    const cgroup = cgroupCommands(helpers.cgroup);
    const observer = {
      cgroup,
      uid,
      closeSender,
      ticksPerSecond,
      checkpoint: () => persist(report),
    };
    for (const [index, trial] of report.trials.entries()) {
      const scenario = scenarios[index % scenarios.length];
      trial.status = "running";
      await persist(report);
      const canContinue = await runTrial(scenario, trial, shim, observer);
      if (!canContinue) {
        report.stopped_after = {
          scenario: trial.scenario,
          iteration: trial.iteration,
          reason: "Owned leaf cleanup could not be confirmed.",
        };
      }
      await persist(report);
      if (!canContinue) break;
    }
  } catch (error) {
    report.infrastructure_failure = describeError(error);
  }
  report.finished_at = new Date().toISOString();
  await persist(report);
  return (
    !report.infrastructure_failure &&
    report.trials.every((trial) => trial.status === "passed")
  );
}

assert.equal(process.platform, "linux", "Use an isolated hosted Linux runner.");
assert(
  process.getuid() > 0 && process.geteuid() === process.getuid(),
  "Run the measurement observer as the non-root runner.",
);
assert(
  process.env.DISPLAY && process.env.DBUS_SESSION_BUS_ADDRESS,
  "Start the measurement under the fixture Xvfb and private D-Bus session.",
);
assert.equal(
  process.argv.length,
  5,
  "Pass the root-owned libexec directory, close sender and output JSON path.",
);
const [, , helpers, closeSender, output] = process.argv;
if (!(await measure(helpers, resolve(closeSender), resolve(output))))
  process.exitCode = 1;
