import assert from "node:assert/strict";
import { test } from "node:test";
import { parseWriteRecoveryError } from "../src/write-recovery.ts";
import nativeSaveAs from "../src-tauri/src/recovery/fixtures/save-as-uncertain.json" with {
  type: "json",
};

const destination = {
  display: "/synthetic/Archive/Fixture 🌟.SC2Bank",
  truncated: false,
  lossy: false,
};
const recorded = {
  kind: "recorded",
  path: {
    display: "/synthetic/Archive/Fixture\uFFFD\r\n.recovery",
    truncated: true,
    lossy: true,
  },
};
const verified = {
  kind: "verified",
  path: {
    display: "/synthetic/Backups/Fixture.SC2Bank.bak",
    truncated: false,
    lossy: false,
  },
};

const receipt = {
  operation: "save_overwrite",
  outcome: "commit_uncertain",
  affectsActive: true,
  destination,
  recovery: recorded,
  backup: verified,
  reason: "Synthetic write verification failure.\nThe operation stopped.",
  reasonTruncated: true,
};

const invalid = { kind: "invalid_receipt" };

function parse(value) {
  return parseWriteRecoveryError({ recovery_required: value });
}

test("write recovery accepts the shared native Save As wire fixture", () => {
  assert.deepEqual(parseWriteRecoveryError(nativeSaveAs), {
    kind: "receipt",
    receipt: nativeSaveAs.recovery_required,
  });
});

test("write recovery preserves operation, outcome, and the exact location evidence", () => {
  for (const value of [
    receipt,
    { ...receipt, operation: "restore" },
    { ...receipt, backup: { kind: "not_created" } },
    {
      ...receipt,
      operation: "save_as",
      affectsActive: false,
      recovery: { kind: "unknown" },
      backup: { kind: "not_created" },
    },
    {
      ...receipt,
      outcome: "live_unchanged",
      affectsActive: false,
      backup: { ...verified, kind: "unverified" },
    },
  ]) {
    assert.deepEqual(parse(value), { kind: "receipt", receipt: value });
  }
});

test("write recovery distinguishes unrelated errors from a malformed recovery tag", () => {
  for (const error of [
    undefined,
    null,
    "active_write_uncertain",
    new Error("Synthetic desktop error"),
    { active_write_rejected: "Synthetic refusal" },
  ]) {
    assert.equal(parseWriteRecoveryError(error), undefined);
  }
  assert.deepEqual(parseWriteRecoveryError("recovery_required"), invalid);
  assert.deepEqual(
    parseWriteRecoveryError({ recovery_required: receipt, extra: true }),
    invalid,
  );
});

test("write recovery rejects malformed paths, flags, and variant members", () => {
  for (const value of [
    undefined,
    null,
    {},
    { ...receipt, operation: "external" },
    { ...receipt, outcome: "written" },
    { ...receipt, affectsActive: 1 },
    { ...receipt, destination: null },
    { ...receipt, destination: { ...destination, lossy: "false" } },
    { ...receipt, destination: { ...destination, display: "\ud800" } },
    { ...receipt, destination: { ...destination, exists: true } },
    { ...receipt, recovery: null },
    { ...receipt, recovery: { kind: "recorded" } },
    { ...receipt, recovery: { ...recorded, kind: "guessed" } },
    { ...receipt, recovery: { ...recorded, kind: "unknown" } },
    { ...receipt, backup: { kind: "verified" } },
    { ...receipt, backup: { ...verified, kind: "not_created" } },
    { ...receipt, reason: 2 },
    { ...receipt, reasonTruncated: undefined },
  ]) {
    assert.deepEqual(parse(value), invalid);
  }
});

test("write recovery enforces every UTF-8 path byte limit and the reason byte limit", () => {
  const path = { ...destination, display: "🌟".repeat(1_024) };
  const reason = "é".repeat(256);
  const bounded = {
    ...receipt,
    destination: path,
    recovery: { ...recorded, path },
    backup: { ...verified, path },
    reason,
  };
  assert.deepEqual(parse(bounded), { kind: "receipt", receipt: bounded });

  const oversizedPath = { ...path, display: `${path.display}/` };
  for (const value of [
    { ...bounded, destination: oversizedPath },
    { ...bounded, recovery: { ...recorded, path: oversizedPath } },
    { ...bounded, backup: { ...verified, path: oversizedPath } },
    { ...bounded, reason: `${reason}!` },
  ]) {
    assert.deepEqual(parse(value), invalid);
  }
});

test("write recovery rejects outcome summaries that contradict their evidence", () => {
  for (const value of [
    { ...receipt, operation: "save_as" },
    { ...receipt, operation: "save_as", affectsActive: false },
    {
      ...receipt,
      operation: "save_as",
      outcome: "live_unchanged",
      affectsActive: false,
      backup: { ...verified, kind: "unverified" },
    },
    { ...receipt, operation: "restore", backup: { kind: "not_created" } },
    { ...receipt, affectsActive: false },
    { ...receipt, backup: { ...verified, kind: "unverified" } },
    { ...receipt, outcome: "live_unchanged" },
    { ...receipt, outcome: "live_unchanged", affectsActive: false },
    {
      ...receipt,
      outcome: "live_unchanged",
      affectsActive: false,
      backup: { kind: "not_created" },
    },
  ]) {
    assert.deepEqual(parse(value), invalid);
  }
});

test("copy recovery never claims active authority or preparatory backups", () => {
  for (const operation of [
    "export_copy",
    "backup_copy",
    "external_signed_copy",
  ]) {
    const copy = { ...nativeSaveAs.recovery_required, operation };
    assert.equal(parse(copy).kind, "receipt");
    for (const invalidCopy of [
      { ...copy, affectsActive: true },
      { ...copy, backup: verified },
      { ...copy, outcome: "live_unchanged" },
    ]) {
      assert.deepEqual(parse(invalidCopy), invalid);
    }
  }
});
