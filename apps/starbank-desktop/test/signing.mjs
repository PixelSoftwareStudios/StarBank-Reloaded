import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  checkExternalBankSignature,
  exportExternalSignedBank,
  pickExternalSigningSource,
  resignActiveBank,
} from "../src/transport.ts";

const target = {
  requestId: "synthetic-signing-request",
  documentId: "synthetic-document",
  revision: "9007199254740993",
};
const context = {
  author: "1-S2-1-1001",
  player: "2-S2-1-2002",
  bank_name: "Synthetic",
};
const fileReply = { name: "Synthetic.sc2bank", control: "synthetic control" };
const invalidResponse = /invalid desktop response/;

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

test("active signing rejects malformed replies before adoption or context preview", async () => {
  for (const value of [
    null,
    {},
    { kind: "verified" },
    { kind: "signed", payload: null },
    { kind: "signed", payload: { ...fileReply, control: 1 } },
    { kind: "signed", payload: { ...fileReply, can_overwrite: "true" } },
    { kind: "checked", payload: "missing_context" },
    { kind: "checked", payload: "verified" },
  ]) {
    reply("resign_active_bank", { target, manual: null }, value);
    await assert.rejects(resignActiveBank(target), invalidResponse);
  }
});

test("active signing retains manual context, exact control text, and checked statuses", async () => {
  for (const value of [
    { kind: "manual_context_required" },
    { kind: "signed", payload: fileReply },
  ]) {
    reply("resign_active_bank", { target, manual: null }, value);
    assert.deepEqual(await resignActiveBank(target), value);
  }
  const manual = { kind: "check", context };
  for (const payload of [
    "unsigned",
    "malformed",
    "unsupported",
    "valid",
    "invalid",
  ]) {
    const value = { kind: "checked", payload };
    reply("resign_active_bank", { target, manual }, value);
    assert.deepEqual(await resignActiveBank(target, manual), value);
  }
});

test("external source rejects malformed context flags and signature states", async () => {
  const source = {
    id: "synthetic-source",
    name: "Synthetic.sc2bank",
    requires_context: true,
    signature_status: "missing_context",
  };
  for (const value of [null, source]) {
    reply("pick_external_bank_to_resign", {}, value);
    assert.deepEqual(await pickExternalSigningSource(), value);
  }
  for (const value of [
    {},
    { ...source, id: "" },
    { ...source, name: 1 },
    { ...source, requires_context: "false" },
    { ...source, signature_status: "verified" },
  ]) {
    reply("pick_external_bank_to_resign", {}, value);
    await assert.rejects(pickExternalSigningSource(), invalidResponse);
  }
});

test("external signature previews reject unknown or missing context results", async () => {
  const id = "synthetic-source";
  for (const value of [
    "unsigned",
    "malformed",
    "unsupported",
    "valid",
    "invalid",
  ]) {
    reply("check_external_bank_signature", { id, context }, value);
    assert.equal(await checkExternalBankSignature(id, context), value);
  }
  for (const value of [null, "missing_context", "verified", { valid: true }]) {
    reply("check_external_bank_signature", { id, context }, value);
    await assert.rejects(
      checkExternalBankSignature(id, context),
      invalidResponse,
    );
  }
});

test("external signing export accepts cancellation and exact returned names only", async () => {
  const id = "synthetic-source";
  for (const value of [null, "Synthetic.sc2bank"]) {
    reply("export_external_signed_bank", { id, context }, value);
    assert.equal(await exportExternalSignedBank(id, context), value);
  }
  reply(
    "export_external_signed_bank",
    { id, context: null },
    { name: "Synthetic.sc2bank" },
  );
  await assert.rejects(exportExternalSignedBank(id, null), invalidResponse);
});

test("native signing rejection retains its failure without a success reply", async () => {
  const failure = new Error("Synthetic external change");
  mockIPC(() => Promise.reject(failure));
  await assert.rejects(resignActiveBank(target), (error) => error === failure);
  await assert.rejects(
    pickExternalSigningSource(),
    (error) => error === failure,
  );
});
