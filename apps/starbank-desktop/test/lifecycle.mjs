import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  confirmWindowClose,
  decodeCloseProbe,
  MAX_NATIVE_U32,
  openRequestedBank,
  readOpenRequests,
} from "../src/transport.ts";

const invalidResponse = /invalid desktop response/;
const report = {
  ticket: 3,
  draftGeneration: 8,
  hasDraft: true,
  canSave: true,
};

beforeEach(() => {
  globalThis.window = {};
});
afterEach(() => {
  clearMocks();
  delete globalThis.window;
});

test("file-open queue retains names and exact native request identifiers", async () => {
  const requests = [
    { id: 1, name: "Synthetic.SC2Bank" },
    { id: MAX_NATIVE_U32, name: "Synthetic 星.SC2Bank" },
  ];
  mockIPC((command, args) => {
    assert.equal(command, "list_open_requests");
    assert.deepEqual(args, {});
    return requests;
  });
  assert.deepEqual(await readOpenRequests(), requests);
});

test("file-open queue rejects malformed names and non-u32 request identifiers", async () => {
  for (const response of [
    null,
    {},
    [null],
    [{ id: 1, name: 7 }],
    ...[0, -1, 0.5, "1", Number.NaN, MAX_NATIVE_U32 + 1].map((id) => [
      { id, name: "Synthetic.SC2Bank" },
    ]),
  ]) {
    mockIPC(() => response);
    await assert.rejects(readOpenRequests(), invalidResponse);
  }
});

test("requested bank adoption validates replies and preserves cancellation", async () => {
  const opened = {
    name: "Synthetic.SC2Bank",
    control: "synthetic-control",
    can_overwrite: true,
    breadcrumbs: ["Synthetic", "Synthetic.SC2Bank"],
  };
  for (const response of [null, opened]) {
    mockIPC((command, args) => {
      assert.equal(command, "open_requested_bank");
      assert.deepEqual(args, { id: 3, control: "synthetic-request" });
      return response;
    });
    assert.deepEqual(await openRequestedBank(3, "synthetic-request"), response);
  }
  for (const response of [
    {},
    { ...opened, name: 3 },
    { ...opened, control: null },
    { ...opened, can_overwrite: "true" },
    { ...opened, breadcrumbs: [false] },
  ]) {
    mockIPC(() => response);
    await assert.rejects(
      openRequestedBank(3, "synthetic-request"),
      invalidResponse,
    );
  }
});

test("close probes ignore malformed events without throwing or rounding tickets", () => {
  for (const ticket of [1, MAX_NATIVE_U32]) {
    assert.deepEqual(decodeCloseProbe({ ticket }), { ticket });
  }
  for (const payload of [
    null,
    undefined,
    {},
    "discard",
    { ticket: 1, proceed: true },
    ...[0, -1, 0.5, "1", Number.NaN, MAX_NATIVE_U32 + 1].map((ticket) => ({
      ticket,
    })),
  ]) {
    assert.equal(decodeCloseProbe(payload), undefined);
  }
});

test("native close confirmation preserves Save, Discard, and Cancel without completing the window", async () => {
  for (const choice of ["save", "discard", "cancel"]) {
    const calls = [];
    mockIPC((command, args) => {
      calls.push(command);
      assert.equal(command, "confirm_window_close");
      assert.deepEqual(args, { report });
      return choice;
    });
    assert.equal(await confirmWindowClose(report), choice);
    assert.deepEqual(calls, ["confirm_window_close"]);
  }
});

test("invalid close choices reject and release the ticket with proceed=false", async () => {
  for (const choice of [
    undefined,
    null,
    true,
    "Save",
    "close",
    { kind: "discard" },
  ]) {
    const completions = [];
    mockIPC((command, args) => {
      if (command === "confirm_window_close") return choice;
      assert.equal(command, "complete_window_close");
      completions.push(args);
      return null;
    });
    await assert.rejects(confirmWindowClose(report), invalidResponse);
    assert.deepEqual(completions, [
      {
        ticket: report.ticket,
        draftGeneration: report.draftGeneration,
        proceed: false,
      },
    ]);
  }
});

test("close confirmation preserves native rejection even when ticket cleanup also fails", async () => {
  const failure = new Error("Synthetic native refusal");
  const commands = [];
  mockIPC((command) => {
    commands.push(command);
    throw command === "confirm_window_close"
      ? failure
      : new Error("Synthetic cleanup refusal");
  });
  await assert.rejects(
    confirmWindowClose(report),
    (error) => error === failure,
  );
  assert.deepEqual(commands, ["confirm_window_close", "complete_window_close"]);
});
