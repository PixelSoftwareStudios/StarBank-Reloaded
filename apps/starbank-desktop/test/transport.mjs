import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  compareBankConflict,
  exportActiveBackup,
  readActiveBackupStatus,
  readInventory,
  readInventoryBackupStatuses,
  reviewActiveBackup,
} from "../src/transport.ts";

const target = {
  requestId: "synthetic-request",
  documentId: "synthetic-document",
  revision: "9007199254740993",
};
const excerpt = { text: "<Bank/>", omitsStart: false, omitsEnd: false };
const comparison = {
  firstDifferenceLine: 1,
  startLine: 1,
  before: excerpt,
  after: { ...excerpt, text: "<Bank version='1'/>" },
};
const backupReview = { description: "Synthetic backup", comparison };
const conflictReview = {
  id: "9007199254740993",
  firstDifferenceLine: comparison.firstDifferenceLine,
  startLine: comparison.startLine,
  editor: comparison.before,
  onDisk: comparison.after,
};
const status = {
  state: "recorded",
  recordCount: 1,
  rejectedMetadata: 0,
  latestTimestampUtc: "2026-11-27T00:00:00Z",
  incomplete: false,
};
const invalidResponse = /invalid desktop response/;

beforeEach(() => {
  globalThis.window = { crypto: globalThis.crypto };
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

test("valid backup and conflict reviews retain exact XML and opaque identifiers", async () => {
  reply("review_active_backup", { target, backupId: 7 }, backupReview);
  assert.deepEqual(await reviewActiveBackup(target, 7), backupReview);
  reply("compare_bank_conflict", { target }, conflictReview);
  assert.deepEqual(await compareBankConflict(target), conflictReview);
  const identical = {
    ...backupReview,
    comparison: { ...comparison, firstDifferenceLine: null, after: excerpt },
  };
  reply("review_active_backup", { target, backupId: 7 }, identical);
  assert.deepEqual(await reviewActiveBackup(target, 7), identical);
});

test("backup review rejects missing, mistyped, and nonpositive comparison data", async () => {
  for (const value of [
    null,
    {},
    { ...backupReview, description: 1 },
    { ...backupReview, comparison: null },
    ...[0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 53].map(
      (startLine) => ({
        ...backupReview,
        comparison: { ...comparison, startLine },
      }),
    ),
    ...[0, -1, "1"].map((firstDifferenceLine) => ({
      ...backupReview,
      comparison: { ...comparison, firstDifferenceLine },
    })),
    {
      ...backupReview,
      comparison: { ...comparison, before: { ...excerpt, text: 7 } },
    },
    {
      ...backupReview,
      comparison: { ...comparison, after: { ...excerpt, omitsEnd: "false" } },
    },
  ]) {
    reply("review_active_backup", { target, backupId: 7 }, value);
    await assert.rejects(reviewActiveBackup(target, 7), invalidResponse);
  }
});

test("conflict review rejects malformed reopen token and XML excerpts", async () => {
  for (const value of [
    null,
    { ...conflictReview, id: "" },
    { ...conflictReview, id: 1 },
    { ...conflictReview, firstDifferenceLine: 0 },
    { ...conflictReview, startLine: -1 },
    { ...conflictReview, editor: null },
    { ...conflictReview, onDisk: { ...excerpt, omitsStart: 0 } },
  ]) {
    reply("compare_bank_conflict", { target }, value);
    await assert.rejects(compareBankConflict(target), invalidResponse);
  }
});

test("active backup status validates every displayed count and status", async () => {
  for (const state of ["recorded", "never_backed_up", "unknown"]) {
    const value = { ...status, state };
    reply("active_backup_status", { target }, value);
    assert.deepEqual(await readActiveBackupStatus(target), value);
  }
  for (const value of [
    null,
    { ...status, state: "verified" },
    ...[-1, 0.5, "1", Number.NaN, 2 ** 53].map((recordCount) => ({
      ...status,
      recordCount,
    })),
    { ...status, rejectedMetadata: -1 },
    { ...status, latestTimestampUtc: 1 },
    { ...status, incomplete: "false" },
  ]) {
    reply("active_backup_status", { target }, value);
    await assert.rejects(readActiveBackupStatus(target), invalidResponse);
  }
});

test("inventory status rejects an invalid row instead of publishing partial history", async () => {
  const rows = [
    { bankId: "synthetic-bank", state: "recorded" },
    { bankId: "another-bank", state: "unknown" },
  ];
  reply("inventory_backup_status", { generation: "1" }, rows);
  assert.deepEqual(await readInventoryBackupStatuses("1"), rows);
  for (const value of [
    null,
    {},
    [rows[0], { bankId: "another-bank", state: "verified" }],
    [{ bankId: 1, state: "recorded" }],
    [null],
  ]) {
    reply("inventory_backup_status", { generation: "1" }, value);
    await assert.rejects(readInventoryBackupStatuses("1"), invalidResponse);
  }
});

test("backup export accepts cancellation but rejects non-text destinations", async () => {
  for (const value of [null, "Synthetic.sc2bank.bak"]) {
    reply("export_active_backup", { target, backupId: 7 }, value);
    assert.equal(await exportActiveBackup(target, 7), value);
  }
  reply("export_active_backup", { target, backupId: 7 }, {});
  await assert.rejects(exportActiveBackup(target, 7), invalidResponse);
});

test("native rejection remains a rejection instead of becoming a review", async () => {
  const failure = new Error("Synthetic stale revision");
  mockIPC(() => Promise.reject(failure));
  await assert.rejects(
    reviewActiveBackup(target, 7),
    (error) => error === failure,
  );
});

const inventory = {
  generation: "9007199254740993",
  status: "ready",
  reason: null,
  maps: [
    {
      id: "synthetic-map",
      name: "Synthetic Map",
      isBlizzard: false,
      hasBankFiles: true,
      author: null,
      modifiedAtMillis: null,
    },
  ],
  banks: [
    {
      id: "synthetic-bank",
      name: "Synthetic.SC2Bank",
      scopeId: null,
      playerHandle: null,
      authorHandle: null,
      mapIds: ["synthetic-map"],
    },
  ],
  rootName: "Synthetic",
  manual: false,
  incomplete: false,
  canRebuildMapIndex: true,
};

function inventoryReply(value) {
  mockIPC((command, { progress }) => {
    assert.equal(command, "refresh_inventory");
    assert.equal(typeof progress.onmessage, "function");
    return value;
  });
}

test("inventory reads validate all four query commands without coercing opaque generations", async () => {
  for (const command of [
    "refresh_inventory",
    "rebuild_map_index",
    "use_automatic_inventory",
    "select_inventory_folder",
  ]) {
    mockIPC((actual, { progress }) => {
      assert.equal(actual, command);
      progress.onmessage({
        generation: inventory.generation,
        stage: "locating_folder",
      });
      return inventory;
    });
    assert.deepEqual(await readInventory(command, () => {}), inventory);
  }
  for (const value of [
    null,
    {
      ...inventory,
      generation: "",
      status: "root-not-found",
      maps: [],
      banks: [],
      reason: "unreadable",
    },
  ]) {
    inventoryReply(value);
    assert.deepEqual(await readInventory("refresh_inventory", () => {}), value);
  }
});

test("inventory snapshots reject malformed maps, bank targets, statuses, and flags before publication", async () => {
  for (const value of [
    {},
    { ...inventory, generation: 1 },
    { ...inventory, status: "scanning" },
    { ...inventory, reason: "unknown_reason" },
    { ...inventory, maps: null },
    { ...inventory, maps: [{ ...inventory.maps[0], isBlizzard: "false" }] },
    {
      ...inventory,
      maps: [{ ...inventory.maps[0], modifiedAtMillis: 2 ** 53 }],
    },
    { ...inventory, banks: [{ ...inventory.banks[0], id: 1 }] },
    {
      ...inventory,
      banks: [{ ...inventory.banks[0], mapIds: "synthetic-map" }],
    },
    { ...inventory, banks: [{ ...inventory.banks[0], scopeId: false }] },
    { ...inventory, canRebuildMapIndex: "false" },
    { ...inventory, manual: null },
  ]) {
    inventoryReply(value);
    await assert.rejects(
      readInventory("refresh_inventory", () => {}),
      invalidResponse,
    );
  }
});

test("inventory progress ignores malformed and foreign updates without poisoning the active generation", async () => {
  const updates = [];
  const valid = {
    generation: inventory.generation,
    stage: "inspecting_archives",
    completed: 1,
    total: 2,
  };
  const invalid = [
    null,
    {},
    { generation: "wrong", stage: "unknown_stage" },
    { stage: "locating_folder" },
    { generation: 1, stage: "locating_folder" },
    ...[-1, 0.5, "1", 2 ** 53].map((completed) => ({
      ...valid,
      generation: "wrong",
      completed,
    })),
    { ...valid, generation: "wrong", total: Number.NaN },
  ];
  mockIPC((_command, { progress }) => {
    for (const update of invalid) progress.onmessage(update);
    progress.onmessage(valid);
    progress.onmessage({ generation: "foreign", stage: "enumerating_banks" });
    return inventory;
  });
  assert.deepEqual(
    await readInventory("refresh_inventory", (value) => updates.push(value)),
    inventory,
  );
  assert.deepEqual(updates, [valid]);
});

test("inventory rejects a snapshot from another progress generation and ignores settled channel updates", async () => {
  const updates = [];
  let send;
  mockIPC((_command, { progress }) => {
    send = progress.onmessage;
    send({ generation: "older-generation", stage: "locating_folder" });
    return inventory;
  });
  await assert.rejects(
    readInventory("refresh_inventory", (value) => updates.push(value)),
    invalidResponse,
  );
  send({ generation: "older-generation", stage: "enumerating_banks" });
  assert.equal(updates.length, 1);
});

test("inventory cancellation and native failures do not keep accepting channel updates", async () => {
  for (const value of [null, new Error("Synthetic native failure")]) {
    const updates = [];
    let send;
    mockIPC((_command, { progress }) => {
      send = progress.onmessage;
      if (value instanceof Error) throw value;
      return value;
    });
    const request = readInventory("refresh_inventory", (update) =>
      updates.push(update),
    );
    if (value instanceof Error)
      await assert.rejects(request, (error) => error === value);
    else assert.equal(await request, null);
    send({ generation: inventory.generation, stage: "locating_folder" });
    assert.deepEqual(updates, []);
  }
});
