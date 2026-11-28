import assert from "node:assert/strict";
import {
  chmod,
  mkdir,
  mkdtempDisposable,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import {
  attributeCpu,
  buildProvenance,
  measurementArguments,
  verifyReleaseExecutable,
} from "./linux-performance.mjs";

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

test("existing measurement arguments preserve debug without requiring a package", async () => {
  const options = measurementArguments(["tools", "close", "output.json"]);
  assert.equal(options.profile, "debug");
  assert.equal(options.output, resolve("output.json"));
  assert.equal(options.packagePath, undefined);
  const provenance = await buildProvenance(options);
  assert.equal(provenance.artifact, null);
  assert.match(provenance.declared_build_command, /--debug/);
});

test("measurement arguments refuse unknown profiles and release without artifact before reading paths", () => {
  for (const profile of ["optimized", "Release", "", "__proto__", "toString"]) {
    assert.throws(
      () =>
        measurementArguments([
          "missing-tools",
          "missing-close",
          "missing-output",
          profile,
        ]),
      /build profile/,
    );
  }
  assert.throws(
    () => measurementArguments(["tools", "close", "output", "release"]),
    /requires the exact verified DEB/,
  );
  assert.throws(
    () => measurementArguments(["tools", "close", "output", "release", ""]),
    /nonempty DEB path/,
  );
  assert.throws(() => measurementArguments(["tools", "close"]), /Pass tools/);
  assert.throws(
    () =>
      measurementArguments([
        "tools",
        "close",
        "out",
        "debug",
        "pkg",
        "extra",
        "overflow",
      ]),
    /Pass tools/,
  );
});

test("release provenance hashes the supplied readonly artifact without claiming optimizer facts", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-perf-provenance-"),
  );
  const packagePath = join(fixture.path, "starbank-reloaded_0.1.0_amd64.deb");
  await writeFile(packagePath, "abc", { flag: "wx", mode: 0o444 });
  const receiptPath = join(fixture.path, "receipt.json");
  await writeFile(receiptPath, JSON.stringify(releaseReceipt()));
  const options = measurementArguments([
    "tools",
    "close",
    "out",
    "release",
    packagePath,
    receiptPath,
  ]);
  const provenance = await buildProvenance(options);
  assert.deepEqual(provenance.artifact, {
    path: packagePath,
    filename: "starbank-reloaded_0.1.0_amd64.deb",
    bytes: 3,
    sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  });
  assert.equal(
    provenance.declared_build_command,
    "pnpm --filter starbank-desktop exec tauri build --bundles deb --no-sign -- --locked --message-format=json-render-diagnostics",
  );
  assert.match(provenance.scope, /CI declarations/);
  assert.match(provenance.scope, /CI-recorded Cargo facts/);
  assert.deepEqual(provenance.build_receipt, releaseReceipt());
});

test("artifact provenance refuses writable, empty, directory and symlink package inputs", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-perf-refusal-"),
  );
  const packagePath = join(fixture.path, "test.deb");
  await writeFile(packagePath, "abc", { flag: "wx", mode: 0o600 });
  await assert.rejects(
    buildProvenance({ profile: "release", packagePath }),
    /read-only regular DEB/,
  );
  await chmod(packagePath, 0o444);
  const alias = join(fixture.path, "alias.deb");
  await symlink(packagePath, alias);
  await assert.rejects(
    buildProvenance({ profile: "release", packagePath: alias }),
    /read-only regular DEB/,
  );
  const empty = join(fixture.path, "empty.deb");
  await writeFile(empty, "", { flag: "wx", mode: 0o444 });
  await assert.rejects(
    buildProvenance({ profile: "release", packagePath: empty }),
    /read-only regular DEB/,
  );
  const directory = join(fixture.path, "directory.deb");
  await mkdir(directory);
  await assert.rejects(
    buildProvenance({ profile: "release", packagePath: directory }),
    /read-only regular DEB/,
  );
});

function releaseReceipt() {
  return {
    schemaVersion: 1,
    buildCommand:
      "pnpm --filter starbank-desktop exec tauri build --bundles deb --no-sign -- --locked --message-format=json-render-diagnostics".split(
        " ",
      ),
    cargoArtifact: {
      profile: { opt_level: "3", debug_assertions: false, test: false },
      target: { name: "starbank-desktop" },
      features: ["desktop", "custom-protocol"],
    },
    deb: {
      name: "starbank-reloaded_0.1.0_amd64.deb",
      size: 3,
      sha256:
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    },
    packagedExecutable: {
      name: "starbank-desktop",
      size: 42,
      sha256: "a".repeat(64),
    },
  };
}

test("release arguments require a receipt and do not accept one for debug", () => {
  assert.throws(
    () =>
      measurementArguments(["tools", "close", "out", "release", "package.deb"]),
    /requires the build receipt/,
  );
  assert.throws(
    () =>
      measurementArguments([
        "tools",
        "close",
        "out",
        "debug",
        "package.deb",
        "receipt.json",
      ]),
    /only valid for release/,
  );
});

test("release provenance refuses mismatched artifact receipts and malformed packaged identities", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-perf-receipt-"),
  );
  const packagePath = join(fixture.path, "starbank-reloaded_0.1.0_amd64.deb");
  const receiptPath = join(fixture.path, "receipt.json");
  await writeFile(packagePath, "abc", { flag: "wx", mode: 0o444 });
  for (const [mutate, error] of [
    [
      (receipt) => {
        receipt.schemaVersion = 2;
      },
      /schema/,
    ],
    [
      (receipt) => {
        receipt.deb.sha256 = "b".repeat(64);
      },
      /DEB identity/,
    ],
    [
      (receipt) => {
        receipt.deb.size = 4;
      },
      /DEB identity/,
    ],
    [
      (receipt) => {
        receipt.deb.name = "other.deb";
      },
      /DEB identity/,
    ],
    [
      (receipt) => {
        receipt.packagedExecutable = undefined;
      },
      /packaged executable/,
    ],
    [
      (receipt) => {
        receipt.packagedExecutable.sha256 = "bad";
      },
      /packaged executable/,
    ],
    [
      (receipt) => {
        receipt.packagedExecutable.size = 0;
      },
      /packaged executable/,
    ],
    [
      (receipt) => {
        receipt.packagedExecutable.name = "other";
      },
      /packaged executable/,
    ],
    [
      (receipt) => {
        receipt.cargoArtifact.profile.debug_assertions = true;
      },
      /Cargo profile/,
    ],
    [
      (receipt) => {
        receipt.buildCommand.push("--debug");
      },
      /release command/,
    ],
  ]) {
    const receipt = releaseReceipt();
    mutate(receipt);
    await writeFile(receiptPath, JSON.stringify(receipt));
    await assert.rejects(
      buildProvenance({ profile: "release", packagePath, receiptPath }),
      error,
    );
  }
});

test("release admission requires the installed payload hash and size to match the receipt", () => {
  const expected = releaseReceipt().packagedExecutable;
  verifyReleaseExecutable(expected, { ...expected });
  for (const change of [
    { sha256: "b".repeat(64) },
    { size: expected.size + 1 },
    { name: "other" },
  ]) {
    assert.throws(
      () => verifyReleaseExecutable(expected, { ...expected, ...change }),
      /Installed executable differs/,
    );
  }
});
