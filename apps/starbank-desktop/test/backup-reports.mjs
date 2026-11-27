import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  createBatchBackup,
  readBackupRetentionReview,
} from "../src/transport.ts";

const invalidResponse = /invalid desktop response/;
const request = {
  scope: { kind: "map", id: "synthetic-map" },
  placement: "managed",
};
const target = {
  requestId: "synthetic",
  documentId: "synthetic-bank",
  revision: "9007199254740993",
};
const outcomes = [
  "created",
  "source_unavailable",
  "rejected",
  "needs_review",
  "cancelled",
  "inventory_changed",
];
const report = {
  entries: outcomes.map((outcome, index) => ({
    bankId: `synthetic-${index}`,
    name: `Synthetic-${index}.SC2Bank`,
    outcome,
    backupName: outcome === "created" ? "Synthetic_20261127.SC2Bank.bak" : null,
  })),
  created: 1,
  failed: 3,
  skipped: 2,
};
const retention = {
  keepNewest: 2,
  verifiedCount: 5,
  retainedCount: 2,
  olderCount: 3,
  missingCount: 0,
  unverifiedCount: 1,
  rejectedMetadata: 0,
  older: [
    {
      name: "Synthetic_20261127.SC2Bank.bak",
      timestampUtc: "2026-11-27T00:00:00Z",
      byteLength: 17,
    },
  ],
};

beforeEach(() => {
  globalThis.window = {};
});
afterEach(() => {
  clearMocks();
  delete globalThis.window;
});

function reply(command, args, value) {
  mockIPC((actualCommand, actualArgs) => {
    assert.equal(actualCommand, command);
    assert.deepEqual(actualArgs, args);
    return value;
  });
}

test("batch reports preserve every known outcome and cancellation without coercion", async () => {
  for (const value of [report, null]) {
    reply("create_batch_backup", request, value);
    assert.deepEqual(await createBatchBackup(request), value);
  }
  const allRequest = { scope: { kind: "all" }, placement: "adjacent" };
  reply("create_batch_backup", allRequest, report);
  assert.deepEqual(await createBatchBackup(allRequest), report);
});

test("batch reports reject malformed rows and unsafe aggregate counts", async () => {
  for (const value of [
    {},
    { ...report, entries: null },
    ...["unknown", null, 1].map((outcome) => ({
      ...report,
      entries: [{ ...report.entries[0], outcome }],
    })),
    { ...report, entries: [{ ...report.entries[0], bankId: 1 }] },
    { ...report, entries: [{ ...report.entries[0], name: null }] },
    { ...report, entries: [{ ...report.entries[0], backupName: true }] },
    ...["created", "failed", "skipped"].flatMap((field) =>
      [-1, 0.5, "1", Number.NaN, 2 ** 53].map((value) => ({
        ...report,
        [field]: value,
      })),
    ),
  ]) {
    reply("create_batch_backup", request, value);
    await assert.rejects(createBatchBackup(request), invalidResponse);
  }
});

test("batch reports cannot claim success or failure totals that disagree with entry outcomes", async () => {
  for (const value of [
    { ...report, created: 2 },
    { ...report, failed: 2, skipped: 3 },
    { ...report, entries: [] },
    {
      entries: [{ ...report.entries[0], outcome: "rejected" }],
      created: 1,
      failed: 0,
      skipped: 0,
    },
  ]) {
    reply("create_batch_backup", request, value);
    await assert.rejects(createBatchBackup(request), invalidResponse);
  }
});

test("retention reports accept a truncated older-backup list and exact metadata", async () => {
  reply("review_backup_retention", { target }, retention);
  assert.deepEqual(await readBackupRetentionReview(target), retention);
  const empty = {
    ...retention,
    verifiedCount: 0,
    retainedCount: 0,
    olderCount: 0,
    older: [],
  };
  reply("review_backup_retention", { target }, empty);
  assert.deepEqual(await readBackupRetentionReview(target), empty);
});

test("retention reports reject malformed counts and older-backup metadata", async () => {
  for (const value of [
    null,
    {},
    { ...retention, keepNewest: 0 },
    { ...retention, keepNewest: 1_001 },
    ...[
      "keepNewest",
      "verifiedCount",
      "retainedCount",
      "olderCount",
      "missingCount",
      "unverifiedCount",
      "rejectedMetadata",
    ].flatMap((field) =>
      [-1, 0.5, "1", Number.NaN, 2 ** 53].map((value) => ({
        ...retention,
        [field]: value,
      })),
    ),
    { ...retention, older: null },
    { ...retention, older: [null] },
    { ...retention, older: [{ ...retention.older[0], name: false }] },
    { ...retention, older: [{ ...retention.older[0], timestampUtc: null }] },
    ...[-1, 0.5, "17", 2 ** 53].map((byteLength) => ({
      ...retention,
      older: [{ ...retention.older[0], byteLength }],
    })),
  ]) {
    reply("review_backup_retention", { target }, value);
    await assert.rejects(readBackupRetentionReview(target), invalidResponse);
  }
});

test("retention summaries reject inconsistent retained, older, and displayed-row counts", async () => {
  for (const value of [
    { ...retention, verifiedCount: 4 },
    { ...retention, retainedCount: 3, verifiedCount: 6 },
    { ...retention, olderCount: 0, verifiedCount: 2 },
    { ...retention, retainedCount: 2, verifiedCount: 1 },
    { ...retention, retainedCount: 1, olderCount: 4 },
  ]) {
    reply("review_backup_retention", { target }, value);
    await assert.rejects(readBackupRetentionReview(target), invalidResponse);
  }
});

test("native backup and retention failures preserve rejection rather than synthesizing reports", async () => {
  const failure = new Error("Synthetic native refusal");
  mockIPC(() => Promise.reject(failure));
  await assert.rejects(
    createBatchBackup(request),
    (error) => error === failure,
  );
  await assert.rejects(
    readBackupRetentionReview(target),
    (error) => error === failure,
  );
});
