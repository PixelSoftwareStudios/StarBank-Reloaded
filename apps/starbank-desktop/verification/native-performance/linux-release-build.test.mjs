import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdir,
  mkdtempDisposable,
  readFile,
  symlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { writeReleaseBuildReceipt } from "./linux-release-build.mjs";

const checker = fileURLToPath(
  new URL("./linux-release-build.mjs", import.meta.url),
);
const app = {
  reason: "compiler-artifact",
  manifest_path: fileURLToPath(
    new URL("../../src-tauri/Cargo.toml", import.meta.url),
  ),
  target: {
    name: "starbank-desktop",
    kind: ["bin"],
    src_path: fileURLToPath(
      new URL("../../src-tauri/src/main.rs", import.meta.url),
    ),
  },
  executable: fileURLToPath(
    new URL("../../../../target/release/starbank-desktop", import.meta.url),
  ),
  profile: {
    opt_level: "3",
    debuginfo: 0,
    debug_assertions: false,
    test: false,
  },
  features: ["custom-protocol", "default", "desktop"],
};
const digest =
  "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

async function writePackagedFixture(directory) {
  const packaged = join(directory, "starbank-desktop");
  await writeFile(packaged, "hello", { mode: 0o444 });
  return packaged;
}

test("CLI records the optimized desktop artifact and exact package and payload bytes", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-build-receipt-"),
  );
  const stdout = join(fixture.path, "build.log");
  const name = "starbank-reloaded_0.1.0_amd64.deb";
  const deb = join(fixture.path, name);
  const output = join(fixture.path, "receipt.json");
  await writeFile(deb, "abc", { mode: 0o444 });
  const packaged = await writePackagedFixture(fixture.path);
  await writeFile(
    stdout,
    [
      "vite building production assets",
      "{ frontend progress }",
      JSON.stringify({ ...app, manifest_path: "/unrelated/Cargo.toml" }),
      JSON.stringify({ ...app, target: { name: "build-script-build" } }),
      JSON.stringify(app),
      JSON.stringify({ reason: "build-finished", success: true }),
    ].join("\r\n"),
  );
  execFileSync(process.execPath, [checker, stdout, deb, packaged, output]);
  const receipt = JSON.parse(await readFile(output, "utf8"));
  assert.equal(receipt.schemaVersion, 1);
  assert.equal(
    receipt.buildCommand.join(" "),
    "pnpm --filter starbank-desktop exec tauri build --bundles deb --no-sign -- --locked --message-format=json-render-diagnostics",
  );
  const { reason: _reason, ...artifact } = app;
  assert.deepEqual(receipt.cargoArtifact, artifact);
  assert.deepEqual(receipt.deb, { name, size: 3, sha256: digest });
  assert.deepEqual(receipt.packagedExecutable, {
    name: "starbank-desktop",
    size: 5,
    sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
  });
  assert.match(receipt.bundleNote, /patch the executable/);
});

test("refuses ambiguous or unqualified app build output before creating a receipt", async (t) => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-build-receipt-"),
  );
  const stdout = join(fixture.path, "build.log");
  const deb = join(fixture.path, "artifact.deb");
  const output = join(fixture.path, "receipt.json");
  await writeFile(deb, "abc", { mode: 0o444 });
  const packaged = await writePackagedFixture(fixture.path);
  const debug = { ...app, profile: { ...app.profile, debug_assertions: true } };
  const cases = [
    ["missing", [], /one desktop compiler artifact/],
    ["duplicate", [app, app], /one desktop compiler artifact/],
    ["mixed profiles", [debug, app], /one desktop compiler artifact/],
    ["debug assertions", [debug], /debug assertions/],
    [
      "wrong optimization",
      [{ ...app, profile: { ...app.profile, opt_level: "0" } }],
      /optimization level 3/,
    ],
    [
      "test executable",
      [{ ...app, profile: { ...app.profile, test: true } }],
      /test mode off/,
    ],
    [
      "wrong target",
      [{ ...app, target: { ...app.target, kind: ["lib"] } }],
      /binary target/,
    ],
    [
      "wrong source",
      [{ ...app, target: { ...app.target, src_path: "main.rs" } }],
      /desktop source/,
    ],
    [
      "wrong path",
      [{ ...app, executable: app.executable.replace("/release/", "/debug/") }],
      /workspace release executable/,
    ],
    [
      "missing custom protocol",
      [{ ...app, features: ["desktop"] }],
      /custom-protocol Cargo features/,
    ],
    [
      "missing desktop",
      [{ ...app, features: ["custom-protocol"] }],
      /custom-protocol Cargo features/,
    ],
  ];
  for (const [name, artifacts, error] of cases) {
    await t.test(name, async () => {
      await writeFile(
        stdout,
        artifacts.map((item) => JSON.stringify(item)).join("\n"),
      );
      await assert.rejects(
        writeReleaseBuildReceipt(stdout, deb, packaged, output),
        error,
      );
      await assert.rejects(readFile(output), { code: "ENOENT" });
    });
  }
});

test("refuses writable, empty, directory, and symlink DEBs before creating a receipt", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-build-receipt-"),
  );
  const stdout = join(fixture.path, "build.log");
  const output = join(fixture.path, "receipt.json");
  const writable = join(fixture.path, "writable.deb");
  const empty = join(fixture.path, "empty.deb");
  const directory = join(fixture.path, "directory.deb");
  const alias = join(fixture.path, "alias.deb");
  const readonly = join(fixture.path, "readonly.deb");
  const packaged = await writePackagedFixture(fixture.path);
  await writeFile(stdout, JSON.stringify(app));
  await writeFile(writable, "abc", { mode: 0o600 });
  await writeFile(empty, "", { mode: 0o444 });
  await mkdir(directory, { mode: 0o700 });
  await writeFile(readonly, "abc", { mode: 0o444 });
  await symlink(readonly, alias);
  for (const path of [writable, empty, directory, alias]) {
    await assert.rejects(
      writeReleaseBuildReceipt(stdout, path, packaged, output),
      /nonempty, read-only regular DEB/,
    );
    await assert.rejects(readFile(output), { code: "ENOENT" });
  }
});

test("never overwrites an existing receipt", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-build-receipt-"),
  );
  const stdout = join(fixture.path, "build.log");
  const output = join(fixture.path, "receipt.json");
  const deb = join(fixture.path, "artifact.deb");
  const packaged = await writePackagedFixture(fixture.path);
  await writeFile(stdout, JSON.stringify(app));
  await writeFile(deb, "abc", { mode: 0o444 });
  await writeFile(output, "existing receipt");
  await assert.rejects(
    writeReleaseBuildReceipt(stdout, deb, packaged, output),
    {
      code: "EEXIST",
    },
  );
  assert.equal(await readFile(output, "utf8"), "existing receipt");
});

test("refuses incorrect or mutable packaged executable paths before creating a receipt", async () => {
  await using fixture = await mkdtempDisposable(
    join(tmpdir(), "starbank-build-receipt-"),
  );
  const stdout = join(fixture.path, "build.log");
  const deb = join(fixture.path, "artifact.deb");
  const output = join(fixture.path, "receipt.json");
  await writeFile(stdout, JSON.stringify(app));
  await writeFile(deb, "abc", { mode: 0o444 });
  const packaged = await writePackagedFixture(fixture.path);
  const unexpected = join(fixture.path, "other-binary");
  await writeFile(unexpected, "hello", { mode: 0o444 });
  const paths = Object.fromEntries(
    ["writable", "empty", "directory", "alias"].map((kind) => [
      kind,
      join(fixture.path, kind, "starbank-desktop"),
    ]),
  );
  for (const kind of Object.keys(paths)) {
    await mkdir(join(fixture.path, kind));
  }
  await writeFile(paths.writable, "hello", { mode: 0o600 });
  await writeFile(paths.empty, "", { mode: 0o444 });
  await mkdir(paths.directory, { mode: 0o555 });
  await symlink(packaged, paths.alias);

  await assert.rejects(
    writeReleaseBuildReceipt(stdout, deb, unexpected, output),
    /Unexpected packaged executable filename/,
  );
  await assert.rejects(readFile(output), { code: "ENOENT" });
  for (const path of Object.values(paths)) {
    await assert.rejects(
      writeReleaseBuildReceipt(stdout, deb, path, output),
      /read-only regular packaged executable/,
    );
    await assert.rejects(readFile(output), { code: "ENOENT" });
  }
});
