import assert from "node:assert/strict";
import { test } from "node:test";
import { attributeCpu } from "./linux-performance.mjs";

const ticksPerSecond = 100;
const app = {
  pid: "42",
  starttime_ticks: "123",
  executable: "/usr/bin/starbank-desktop",
  effective_uid: "1001",
  user_cpu_ticks: "18446744073709551400",
  system_cpu_ticks: "9007199254740993",
};

function start(members) {
  return {
    request_started_ms_after_readiness: 1_000,
    request_finished_ms_after_readiness: 1_020,
    members,
  };
}

function end(members) {
  return {
    request_started_ms_after_readiness: 4_000,
    request_finished_ms_after_readiness: 4_060,
    members,
  };
}

test("attributes exact CPU tick differences using the membership midpoint span", () => {
  const result = attributeCpu(
    start([app]),
    end([
      {
        ...app,
        user_cpu_ticks: "18446744073709551499",
        system_cpu_ticks: "9007199254740994",
      },
    ]),
    ticksPerSecond,
  );
  assert.deepEqual(result, {
    status: "matched_endpoints",
    membership_midpoint_span_ms: 3_020,
    matched: [
      {
        pid: app.pid,
        starttime_ticks: app.starttime_ticks,
        executable: app.executable,
        effective_uid: app.effective_uid,
        user_cpu_delta_ticks: 99,
        system_cpu_delta_ticks: 1,
        cpu_delta_ms: 1_000,
        cpu_percent_of_one_core: (1_000 / 3_020) * 100,
      },
    ],
    start_only: [],
    end_only: [],
  });
});

test("does not attribute PID reuse or changed process identity", () => {
  for (const change of [
    { starttime_ticks: "124" },
    { executable: "/usr/libexec/WebKitWebProcess" },
    { effective_uid: "1002" },
  ]) {
    const result = attributeCpu(
      start([app]),
      end([{ ...app, ...change, user_cpu_ticks: "0" }]),
      ticksPerSecond,
    );
    assert.equal(result.status, "partial");
    assert.deepEqual(result.matched, []);
    const identity = {
      pid: app.pid,
      starttime_ticks: app.starttime_ticks,
      executable: app.executable,
      effective_uid: app.effective_uid,
    };
    assert.deepEqual(result.start_only, [identity]);
    assert.deepEqual(result.end_only, [{ ...identity, ...change }]);
  }
});

test("marks attribution unavailable when either endpoint failed", () => {
  const failure = { failure: { message: "Synthetic membership churn" } };
  assert.deepEqual(attributeCpu(failure, end([app]), ticksPerSecond), {
    status: "unavailable",
  });
  assert.deepEqual(attributeCpu(start([app]), failure, ticksPerSecond), {
    status: "unavailable",
  });
});

test("rejects backwards user or system CPU ticks for a matched process", () => {
  for (const counter of ["user_cpu_ticks", "system_cpu_ticks"]) {
    assert.throws(
      () =>
        attributeCpu(
          start([app]),
          end([{ ...app, [counter]: "0" }]),
          ticksPerSecond,
        ),
      /CPU tick counter moved backwards/,
    );
  }
});

test("rejects a combined tick delta outside the safe integer range", () => {
  assert.throws(
    () =>
      attributeCpu(
        start([{ ...app, user_cpu_ticks: "0", system_cpu_ticks: "0" }]),
        end([
          { ...app, user_cpu_ticks: "9007199254740991", system_cpu_ticks: "1" },
        ]),
        ticksPerSecond,
      ),
    /CPU tick delta is out of range/,
  );
});
