import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  canCreateBankCopy,
  canExportBank,
  canRevealBank,
} from "../src/transport.ts";

beforeEach(() => {
  globalThis.window = {};
});
afterEach(() => {
  clearMocks();
  delete globalThis.window;
});

for (const [command, read] of [
  ["can_create_bank_copy", canCreateBankCopy],
  ["can_export_bank", canExportBank],
  ["can_reveal_bank", canRevealBank],
]) {
  test(`${command} accepts only explicit native booleans`, async () => {
    for (const value of [true, false]) {
      mockIPC((actualCommand) => {
        assert.equal(actualCommand, command);
        return value;
      });
      assert.equal(await read(), value);
    }
    for (const value of [null, undefined, "false", 0, 1, {}, []]) {
      mockIPC(() => value);
      await assert.rejects(read(), /invalid desktop response/);
    }
  });
}

test("unavailable native capability checks remain rejected", async () => {
  const failure = new Error("Synthetic unavailable native host");
  mockIPC(() => Promise.reject(failure));
  await assert.rejects(canCreateBankCopy(), (error) => error === failure);
  await assert.rejects(canExportBank(), (error) => error === failure);
  await assert.rejects(canRevealBank(), (error) => error === failure);
});
