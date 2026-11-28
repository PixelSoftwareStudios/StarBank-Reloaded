import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, readFile, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const workspace = fileURLToPath(new URL("../../../../", import.meta.url));
const appName = "starbank-desktop";
const desktop = join(workspace, "apps", appName, "src-tauri");
const manifest = join(desktop, "Cargo.toml");
const source = join(desktop, "src", "main.rs");
const executable = join(workspace, "target", "release", appName);
export const releaseBuildCommand = Object.freeze([
  "pnpm",
  "--filter",
  appName,
  "exec",
  "tauri",
  "build",
  "--bundles",
  "deb",
  "--no-sign",
  "--",
  "--locked",
  "--message-format=json-render-diagnostics",
]);

function appArtifact(log) {
  const candidates = [];
  for (const line of log.split("\n")) {
    if (!line.trimStart().startsWith("{")) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      // Frontend build output shares stdout with Cargo.
      continue;
    }
    if (
      message?.reason === "compiler-artifact" &&
      message.manifest_path === manifest &&
      message.target?.name === appName
    ) {
      candidates.push(message);
    }
  }
  assert.equal(candidates.length, 1, "Expected one desktop compiler artifact.");
  const artifact = candidates[0];
  assert.deepEqual(artifact.target.kind, ["bin"], "Expected a binary target.");
  assert.equal(artifact.target.src_path, source, "Unexpected desktop source.");
  assert.equal(
    artifact.executable,
    executable,
    "Expected the workspace release executable.",
  );
  assert(
    artifact.profile?.opt_level === "3" &&
      artifact.profile.debug_assertions === false &&
      artifact.profile.test === false,
    "Expected optimization level 3 with debug assertions and test mode off.",
  );
  assert(
    Array.isArray(artifact.features) &&
      artifact.features.every((feature) => typeof feature === "string") &&
      artifact.features.includes("desktop") &&
      artifact.features.includes("custom-protocol"),
    "Expected the desktop and custom-protocol Cargo features.",
  );
  return {
    manifest_path: artifact.manifest_path,
    target: artifact.target,
    executable: artifact.executable,
    profile: artifact.profile,
    features: artifact.features,
  };
}

function sameFile(before, after) {
  return (
    after.isFile() &&
    after.dev === before.dev &&
    after.ino === before.ino &&
    after.mode === before.mode &&
    after.size === before.size &&
    after.mtimeNs === before.mtimeNs &&
    after.ctimeNs === before.ctimeNs
  );
}

async function readArtifact(path, description) {
  const before = await lstat(path, { bigint: true });
  assert(
    before.isFile() && before.size > 0n && (before.mode & 0o222n) === 0n,
    `Expected a nonempty, read-only regular ${description}.`,
  );
  const size = Number(before.size);
  assert(
    Number.isSafeInteger(size),
    `Could not report ${description} size exactly.`,
  );
  await using file = await open(
    path,
    constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK,
  );
  const opened = await file.stat({ bigint: true });
  assert(
    sameFile(before, opened),
    `The ${description} changed before hashing.`,
  );
  const hash = createHash("sha256");
  for await (const chunk of file.createReadStream({ autoClose: false })) {
    hash.update(chunk);
  }
  const after = await lstat(path, { bigint: true });
  assert(sameFile(opened, after), `The ${description} changed during hashing.`);
  return { name: basename(path), size, sha256: hash.digest("hex") };
}

export async function readReleaseDeb(path) {
  assert(basename(path).endsWith(".deb"), "Expected a DEB artifact path.");
  return readArtifact(path, "DEB artifact");
}

export async function writeReleaseBuildReceipt(
  stdoutPath,
  debPath,
  packagedExecutablePath,
  receiptPath,
) {
  const artifact = appArtifact(await readFile(stdoutPath, "utf8"));
  const deb = await readReleaseDeb(debPath);
  assert.equal(
    basename(packagedExecutablePath),
    appName,
    "Unexpected packaged executable filename.",
  );
  const packagedExecutable = await readArtifact(
    packagedExecutablePath,
    "packaged executable",
  );
  const receipt = {
    schemaVersion: 1,
    buildCommand: releaseBuildCommand,
    cargoArtifact: artifact,
    deb,
    packagedExecutable,
    bundleNote:
      "Tauri can patch the executable with the bundle type before packaging and restore the Cargo output afterward.",
  };
  await writeFile(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`, {
    flag: "wx",
  });
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    assert.equal(
      process.argv.length,
      6,
      "Usage: linux-release-build.mjs <cargo-stdout> <readonly-deb> <readonly-packaged-executable> <new-receipt>",
    );
    await writeReleaseBuildReceipt(...process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
