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

const verifiedWrite = { ...receipt, outcome: "verified_needs_reopen" };
const editorTarget = {
  document_id: "document-42",
  expected_revision: "9007199254740993",
};
const verifiedSaveAs = {
  ...verifiedWrite,
  operation: "save_as",
  affectsActive: false,
  backup: { kind: "not_created" },
};

test("verified write receipts preserve the optional editor target and physical write scope", () => {
  for (const value of [
    verifiedWrite,
    { ...verifiedWrite, editorTarget },
    { ...verifiedWrite, backup: { kind: "not_created" } },
    { ...verifiedWrite, operation: "restore", editorTarget },
    verifiedSaveAs,
    { ...verifiedSaveAs, editorTarget },
  ]) {
    assert.deepEqual(parse(value), { kind: "receipt", receipt: value });
  }
});

test("verified write receipts reject unrelated operations and contradictory backup evidence", () => {
  for (const value of [
    { ...verifiedWrite, affectsActive: false },
    { ...verifiedWrite, backup: { ...verified, kind: "unverified" } },
    { ...verifiedWrite, operation: "restore", backup: { kind: "not_created" } },
    { ...verifiedSaveAs, affectsActive: true },
    { ...verifiedSaveAs, backup: verified },
    ...["export_copy", "backup_copy", "external_signed_copy"].map(
      (operation) => ({ ...verifiedSaveAs, operation }),
    ),
  ]) {
    assert.deepEqual(parse(value), invalid);
  }
});

test("verified editor targets enforce opaque ID bounds and canonical u64 revisions", () => {
  const documentId = "🌟".repeat(32);
  for (const revision of ["0", "9007199254740993", "18446744073709551615"]) {
    const value = {
      ...verifiedWrite,
      editorTarget: { document_id: documentId, expected_revision: revision },
    };
    assert.deepEqual(parse(value), { kind: "receipt", receipt: value });
  }

  for (const target of [
    undefined,
    null,
    {},
    "document-42",
    { ...editorTarget, document_id: "" },
    { ...editorTarget, document_id: 42 },
    { ...editorTarget, document_id: "doc\u0000" },
    { ...editorTarget, document_id: "doc\ud800" },
    { ...editorTarget, document_id: `${documentId}x` },
    { ...editorTarget, revision: editorTarget.expected_revision },
    ...[
      "",
      "00",
      "+1",
      "-1",
      " 1",
      "1\n",
      "1.0",
      "18446744073709551616",
      1,
    ].map((expected_revision) => ({ ...editorTarget, expected_revision })),
  ]) {
    assert.deepEqual(
      parse({ ...verifiedWrite, editorTarget: target }),
      invalid,
    );
  }
});

test("editor targets are reserved for verified writes that need to reopen", () => {
  for (const value of [
    { ...receipt, editorTarget },
    {
      ...receipt,
      outcome: "live_unchanged",
      affectsActive: false,
      backup: { ...verified, kind: "unverified" },
      editorTarget,
    },
  ]) {
    assert.deepEqual(parse(value), invalid);
  }
});
