import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import init, { WasmBankEngine } from "../src/generated/starbank_wasm.js";
import { verifyBatchBackupReview } from "./batch-backup-review.mjs";
import { verifyEmptyEngineCloseProbe } from "./native-close-probe.mjs";
import { verifyNativeStructureDraftClose } from "./structure-draft-close.mjs";
import { verifyVerifiedNeedsReopen } from "./verified-needs-reopen.mjs";

const dist = fileURLToPath(
  new URL("../../starbank-desktop/dist", import.meta.url),
);
const buildPlatformLabel =
  new Map([
    ["darwin", "macOS"],
    ["macos", "macOS"],
    ["linux", "Linux"],
    ["windows", "Windows"],
    ["win32", "Windows"],
  ]).get(process.env.TAURI_ENV_PLATFORM ?? process.platform) ?? "Desktop";
const origin = "https://starbank-desktop.test";
const wasm = new URL("../src/generated/starbank_wasm_bg.wasm", import.meta.url);
const initialName = "Synthetic.SC2Bank";
const initialBank = Buffer.from(
  '<Bank version="1"><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section></Bank>',
);
const verifiedBackupId = 1;
const conflictReviewId = "1";
const changedDiskBank = Buffer.from(
  initialBank.toString().replace('"001"', '"006"'),
);
const restoredBank = Buffer.from(
  initialBank.toString().replace('"001"', '"003"'),
);
const storageRefusal = "Synthetic storage refusal.";
const savedCopyName = "Saved Copy.SC2Bank";
const exportedCopyName = "Exported Copy.SC2Bank";
const signedCopyName = "Signed Copy.SC2Bank";
const externalSigningSourceId = "fixture-external-bank";
const frameHeaderBytes = 5;
const controlLengthOffset = 1;
const settings = {
  version: 1,
  hideBlizzardMaps: true,
  hideMapsWithoutBanks: true,
  autoResignOnSave: true,
  theme: "system",
  density: "compact",
  textSize: "normal",
  mapPaneWidth: 28,
  sectionsLayout: "separate",
  showBreadcrumbs: true,
  wrapRaw: false,
  groupNumbers: true,
  automaticBackup: "first-overwrite",
};
const inventory = {
  generation: "17",
  status: "ready",
  reason: null,
  canRebuildMapIndex: false,
  rootName: "Synthetic",
  manual: false,
  incomplete: false,
  maps: [
    {
      id: "map-a",
      name: "Synthetic Map",
      isBlizzard: false,
      hasBankFiles: true,
      author: null,
      modifiedAtMillis: null,
    },
  ],
  banks: [
    {
      id: "bank-a",
      name: initialName,
      scopeId: null,
      playerHandle: null,
      authorHandle: null,
      mapIds: ["map-a"],
    },
  ],
};
const blockedInventory = {
  ...inventory,
  generation: "16",
  status: "failed",
  reason: "configuration_unavailable",
  rootName: null,
  incomplete: true,
  maps: [],
  banks: [],
};
const backups = {
  records: [
    {
      id: 0,
      name: "Unavailable.SC2Bank",
      timestamp: "2026-11-27 00:00:00",
      placement: "managed",
      health: "missing",
    },
    {
      id: verifiedBackupId,
      name: "Verified.SC2Bank",
      timestamp: "2026-11-26 00:00:00",
      placement: "managed",
      health: "verified",
    },
  ],
  rejectedMetadata: 0,
  unavailableLocations: 0,
};

function dispatch(engine, control, bytes) {
  const reply = engine.dispatch(control, bytes);
  try {
    return { control: reply.take_control(), bytes: reply.take_bytes() };
  } finally {
    reply.free();
  }
}

function dispatchFrame(engine, packet, beforeDispatch, afterDispatch) {
  const frame = Buffer.from(packet);
  const controlEnd = frameHeaderBytes + frame.readUInt32LE(controlLengthOffset);
  assert(controlEnd <= frame.length);
  const attached = frame[0] === 1;
  assert(attached || (frame[0] === 0 && controlEnd === frame.length));
  const request = frame.subarray(frameHeaderBytes, controlEnd).toString();
  const parsed = JSON.parse(request);
  beforeDispatch(parsed);
  const actual = dispatch(
    engine,
    request,
    attached ? frame.subarray(controlEnd) : undefined,
  );
  const reply = afterDispatch(parsed, actual);

  const control = Buffer.from(reply.control);
  const output = Buffer.alloc(
    frameHeaderBytes + control.length + (reply.bytes?.length ?? 0),
  );
  output[0] = reply.bytes === undefined ? 0 : 1;
  output.writeUInt32LE(control.length, controlLengthOffset);
  control.copy(output, frameHeaderBytes);
  if (reply.bytes) output.set(reply.bytes, frameHeaderBytes + control.length);
  return Array.from(output);
}

function createBridge(engine, canOverwrite, copyCapability) {
  const calls = [];
  const unexpected = [];
  let navigation;
  let openCount = 0;
  let openName = initialName;
  let saved = false;
  let malformedBackupList = false;
  let supersedeNextEdit = false;
  let failNextEditProjection = false;
  let failNextOpenProjection = false;
  let restoreNextBackup = false;
  let nextWriteRecovery;
  let cancelPicker = false;
  let heldPicker;
  let closeChoice;
  let heldCloseConfirmation;
  let heldStructureReply;
  let failedOpenTarget;
  let failedProjectionRevision;
  const wireRequests = [];
  let inventoryResponse = canOverwrite ? blockedInventory : inventory;
  let preferences = { ...settings };

  function dirtySnapshot(target) {
    const reply = JSON.parse(
      dispatch(
        engine,
        JSON.stringify({
          schema_version: 1,
          request_id: target.requestId,
          document_id: target.documentId,
          expected_revision: target.revision,
          command: { kind: "snapshot" },
        }),
      ).control,
    );
    assert.equal(reply.result.kind, "snapshot");
    assert.equal(reply.result.payload.dirty, true);
    return reply;
  }

  function advanceRevisionBeforeEdit(request) {
    if (request.command.kind !== "set_values") return;
    supersedeNextEdit = false;
    const { control } = dispatch(
      engine,
      JSON.stringify({ ...request, command: { kind: "reset" } }),
    );
    const reset = JSON.parse(control);
    assert.equal(reset.result.kind, "snapshot");
    assert.notEqual(reset.revision, request.expected_revision);
  }

  function transformReply(request, reply) {
    if (
      failedOpenTarget &&
      request.command.kind === "raw_text" &&
      request.document_id === failedOpenTarget.document_id &&
      request.expected_revision === failedOpenTarget.expected_revision
    ) {
      const accepted = JSON.parse(reply.control);
      assert.equal(accepted.result.kind, "raw_text");
      failedOpenTarget = undefined;
      return {
        ...reply,
        control: JSON.stringify({ ...accepted, revision: "999" }),
      };
    }
    if (failNextEditProjection && request.command.kind === "set_values") {
      const accepted = JSON.parse(reply.control);
      assert.equal(accepted.result.kind, "snapshot");
      failedProjectionRevision = accepted.revision;
      failNextEditProjection = false;
    }
    if (
      failedProjectionRevision !== undefined &&
      request.command.kind === "raw_text" &&
      request.expected_revision === failedProjectionRevision
    ) {
      failedProjectionRevision = undefined;
      return { control: "unreadable fixture control" };
    }
    return reply;
  }

  function acceptBankOpen(request, bytes) {
    const reply = dispatch(engine, JSON.stringify(request), bytes);
    if (failNextOpenProjection) {
      failNextOpenProjection = false;
      const accepted = JSON.parse(reply.control);
      assert.equal(accepted.result.kind, "snapshot");
      failedOpenTarget = {
        document_id: accepted.document_id,
        expected_revision: accepted.revision,
      };
    }
    return {
      value: {
        name: openName,
        can_overwrite: canOverwrite,
        control: reply.control,
      },
    };
  }

  function replaceFromDisk(target, bytes) {
    dirtySnapshot(target);
    return acceptBankOpen(
      {
        schema_version: 1,
        request_id: target.requestId,
        command: { kind: "open", payload: { discard_current: true } },
      },
      bytes,
    );
  }

  function invoke(command, args) {
    if (command === "dispatch_bank") {
      let hold;
      const reply = {
        value: dispatchFrame(
          engine,
          args,
          (request) => {
            wireRequests.push(request);
            if (request.command.kind === "structure") {
              hold = heldStructureReply;
              heldStructureReply = undefined;
            }
            if (supersedeNextEdit) advanceRevisionBeforeEdit(request);
          },
          transformReply,
        ),
      };
      if (!hold) return reply;
      hold.start();
      return hold.wait.then(() => reply);
    }
    calls.push({ command, args });
    if (nextWriteRecovery?.command === command) {
      const receipt = nextWriteRecovery.receipt;
      nextWriteRecovery = undefined;
      return {
        error: {
          recovery_required:
            typeof receipt === "function" ? receipt() : receipt,
        },
      };
    }
    switch (command) {
      case "confirm_window_close": {
        assert(closeChoice);
        const reply = { value: closeChoice };
        const held = heldCloseConfirmation;
        heldCloseConfirmation = undefined;
        return held ? held.then(() => reply) : reply;
      }
      case "complete_window_close":
        return {};
      case "read_settings":
        return {
          value: {
            settings: preferences,
            legacyRootPending: false,
            automaticBackupAvailable: canOverwrite,
          },
        };
      case "change_setting":
        assert.equal(args.change.key, "mapIndexWorkers");
        assert(["auto", "1", "2", "3", "4"].includes(args.change.value));
        preferences = {
          ...preferences,
          mapIndexWorkers: args.change.value,
        };
        return { value: null };
      case "read_backup_preferences":
        return { value: { location: { mode: "default" }, keepNewest: null } };
      case "list_open_requests":
        return { value: [] };
      case "can_export_bank":
        return { value: copyCapability === undefined };
      case "can_create_bank_copy":
        return copyCapability ?? { value: true };
      case "can_reveal_bank":
        return { value: false };
      case "refresh_inventory":
        return { value: inventoryResponse };
      case "select_inventory_folder":
        return { value: null };
      case "use_automatic_inventory":
        inventoryResponse = inventory;
        return { value: inventoryResponse };
      case "inventory_backup_status":
        return { value: [{ bankId: "bank-a", state: "never_backed_up" }] };
      case "plugin:event|listen":
        return { value: args.handler };
      case "plugin:event|unlisten":
        return {};
      case "open_bank": {
        if (heldPicker) {
          const result = heldPicker;
          heldPicker = undefined;
          return result;
        }
        if (cancelPicker) {
          cancelPicker = false;
          return { value: null };
        }
        const request = JSON.parse(args.control);
        // The WASM wire protocol leaves native save acknowledgement to the host.
        // After a synthetic acknowledgement, let WASM replace the saved session.
        if (saved) {
          assert.equal(request.command.payload?.discard_current, undefined);
          request.command.payload = { discard_current: true };
          saved = false;
        }
        openCount += 1;
        openName =
          openCount === 1 ? initialName : `Replacement-${openCount}.SC2Bank`;
        return acceptBankOpen(request, initialBank);
      }
      case "active_backup_status":
        return {
          value: {
            state: "recorded",
            recordCount: backups.records.length,
            rejectedMetadata: 0,
            latestTimestampUtc: null,
            incomplete: false,
          },
        };
      case "list_active_backups":
        dirtySnapshot(args.target);
        if (malformedBackupList) {
          return { value: { ...backups, unavailableLocations: -1 } };
        }
        return { value: backups };
      case "export_active_backup":
        dirtySnapshot(args.target);
        return { value: "Backup Copy.SC2Bank" };
      case "export_bank_copy": {
        const { control, bytes } = dispatch(engine, args.control);
        assert.equal(JSON.parse(control).result.kind, "prepared");
        assert.deepEqual(
          Buffer.from(bytes),
          Buffer.from(initialBank.toString().replace('"001"', '"002"')),
        );
        return { value: { name: exportedCopyName, control } };
      }
      case "pick_external_bank_to_resign":
        return {
          value: {
            id: externalSigningSourceId,
            name: "External.SC2Bank",
            requires_context: false,
            signature_status: "valid",
          },
        };
      case "export_external_signed_bank":
        return { value: signedCopyName };
      case "discard_external_signing_source":
        return {};
      case "review_active_backup":
        dirtySnapshot(args.target);
        return {
          value: {
            description:
              "A reversal backup will be verified first. This discards the unsaved changes.",
            comparison: {
              firstDifferenceLine: 1,
              startLine: 1,
              before: {
                text: initialBank.toString(),
                omitsStart: false,
                omitsEnd: false,
              },
              after: {
                text: initialBank.toString().replace('"001"', '"003"'),
                omitsStart: false,
                omitsEnd: false,
              },
            },
          },
        };
      case "restore_active_bank":
        assert.equal(args.backupId, verifiedBackupId);
        if (restoreNextBackup) {
          restoreNextBackup = false;
          return replaceFromDisk(args.target, restoredBank);
        }
        dirtySnapshot(args.target);
        return { value: null };
      case "compare_bank_conflict":
        dirtySnapshot(args.target);
        return {
          value: {
            id: conflictReviewId,
            firstDifferenceLine: 1,
            startLine: 1,
            editor: {
              text: initialBank.toString().replace('"001"', '"004"'),
              omitsStart: false,
              omitsEnd: false,
            },
            onDisk: {
              text: changedDiskBank.toString(),
              omitsStart: false,
              omitsEnd: false,
            },
          },
        };
      case "reopen_bank_conflict":
        assert.equal(args.reviewId, conflictReviewId);
        return replaceFromDisk(args.target, changedDiskBank);
      case "confirm_bank_navigation":
        assert(navigation);
        dirtySnapshot(args.target);
        return { value: navigation.choice };
      case "save_active_bank":
      case "save_bank_as": {
        const snapshot = dirtySnapshot(args.target);
        if (navigation.outcome === "cancel") return { value: null };
        if (navigation.outcome === "fail")
          return { error: { active_write_rejected: storageRefusal } };
        if (navigation.outcome === "external")
          return { error: "external_bank_changed" };
        assert.equal(navigation.outcome, "success");
        snapshot.result.payload.dirty = false;
        saved = true;
        return {
          value: {
            name: canOverwrite ? initialName : savedCopyName,
            can_overwrite: canOverwrite,
            control: JSON.stringify(snapshot),
          },
        };
      }
      default:
        unexpected.push(command);
        throw new Error(`Unexpected synthetic Tauri command ${command}`);
    }
  }

  return {
    invoke,
    unexpected,
    choose(choice, outcome) {
      navigation = { choice, outcome };
    },
    supersedeNextEdit() {
      supersedeNextEdit = true;
    },
    failNextEditProjection() {
      failNextEditProjection = true;
    },
    failNextOpenProjection() {
      failNextOpenProjection = true;
    },
    acceptNextRestore() {
      restoreNextBackup = true;
    },
    holdNextPicker() {
      const { promise, resolve } = Promise.withResolvers();
      heldPicker = promise;
      return () => resolve({ value: null });
    },
    chooseClose(choice) {
      closeChoice = choice;
    },
    holdNextCloseConfirmation() {
      const { promise, resolve } = Promise.withResolvers();
      heldCloseConfirmation = promise;
      return resolve;
    },
    holdNextStructureReply() {
      const start = Promise.withResolvers();
      const reply = Promise.withResolvers();
      heldStructureReply = { start: start.resolve, wait: reply.promise };
      return { started: start.promise, release: reply.resolve };
    },
    cancelNextPicker() {
      cancelPicker = true;
    },
    dispatchFixture(request) {
      return JSON.parse(dispatch(engine, JSON.stringify(request)).control);
    },
    failNextWrite(command, receipt) {
      assert.equal(nextWriteRecovery, undefined);
      nextWriteRecovery = { command, receipt };
    },
    wireRequests,
    corruptBackupList() {
      malformedBackupList = true;
    },
    callsFor(command) {
      return calls
        .filter((call) => call.command === command)
        .map((call) => call.args);
    },
  };
}

async function bindRenderer(page, bridge) {
  await page.exposeFunction("invokeDesktopFixture", bridge.invoke);
  await page.addInitScript(() => {
    let callbackId = 0;
    const callbacks = new Map();
    window.fixtureNavigationReplies = 0;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) {
        callbackId += 1;
        callbacks.set(callbackId, callback);
        return callbackId;
      },
      unregisterCallback(id) {
        callbacks.delete(id);
      },
      async invoke(command, args) {
        const binary = command === "dispatch_bank";
        if (
          command === "plugin:event|listen" &&
          args.event === "bank-close-requested"
        ) {
          window.fixtureCloseProbe = (payload) =>
            callbacks.get(args.handler)({
              event: args.event,
              id: args.handler,
              payload,
            });
        }
        if (command === "confirm_window_close")
          window.fixtureRequestedCloseTicket = args.report.ticket;
        const reply = await window.invokeDesktopFixture(
          command,
          binary ? Array.from(args) : args,
        );
        if (command === "confirm_bank_navigation")
          window.fixtureNavigationReplies += 1;
        if (command === "can_create_bank_copy")
          window.fixtureCopyCapabilityReplied = true;
        if (command === "save_bank_as")
          window.fixtureSaveAsReplies = (window.fixtureSaveAsReplies ?? 0) + 1;
        if (command === "complete_window_close")
          window.fixtureCompletedCloseTicket = args.ticket;
        if (reply.error) throw reply.error;
        return binary ? Uint8Array.from(reply.value).buffer : reply.value;
      },
    };
  });
}

function button(page, name) {
  return page.getByRole("button", { name, exact: true });
}

function goldField(page) {
  return page.getByRole("table").getByRole("textbox");
}

async function editGold(page, value) {
  await goldField(page).fill(value);
  await button(page, "Apply edits").click();
  await button(page, "Undo").click({ trial: true });
}

async function verifyRevert(page) {
  const status = page
    .getByRole("status")
    .filter({ hasText: "Unsaved changes in 1 section" });
  const section = page
    .getByRole("navigation", { name: "Bank sections" })
    .getByRole("button", { name: "Player", exact: true });
  const more = button(page, "More save options");
  const menu = page.getByRole("menu", { name: "Save options" });
  const revert = page.getByRole("menuitem", {
    name: "Revert to Last Save",
    exact: true,
  });
  await status.waitFor();
  await section.locator(".editor-section-dirty").waitFor();
  assert.equal(
    await section.getAttribute("aria-description"),
    "Unsaved changes",
  );

  for (const approve of [false, true]) {
    await more.click({ trial: true });
    await more.press("ArrowDown");
    const decision = page.waitForEvent("dialog").then(async (dialog) => {
      assert.equal(dialog.type(), "confirm");
      assert.equal(
        dialog.message(),
        "Replace the unsaved edits with the last saved bank?",
      );
      if (approve) await dialog.accept();
      else await dialog.dismiss();
    });
    await Promise.all([decision, revert.press("Enter")]);
    await menu.waitFor({ state: "hidden" });
    if (!approve) assert.equal(await goldField(page).inputValue(), "002");
  }
  await page
    .getByRole("status")
    .filter({ hasText: "Reverted to the last save." })
    .waitFor();
  assert.equal(await goldField(page).inputValue(), "001");
  assert.equal(await section.getAttribute("aria-description"), null);
  assert.equal(await status.count(), 0);
  await button(page, "Undo").click();
  await status.waitFor();
  assert.equal(await goldField(page).inputValue(), "002");
}

async function openBank(page) {
  await button(page, "Open Bank…").click();
  await page.getByRole("heading", { name: initialName, exact: true }).waitFor();
}

async function verifyInventoryRecovery(page, bridge) {
  await openBank(page);
  await goldField(page).fill("002");
  await button(page, "Settings").click();
  const settingsPanel = page.getByRole("region", { name: "Settings" });
  const blocked = settingsPanel.getByRole("status").filter({
    hasText: "The saved bank folder could not be read.",
  });
  await blocked.waitFor();
  assert.equal(await button(settingsPanel, "Refresh").count(), 0);
  assert.equal(await button(settingsPanel, "Rebuild map index").count(), 0);
  const workers = settingsPanel.getByRole("combobox", {
    name: "Map index workers",
    exact: true,
  });
  assert.equal(await workers.inputValue(), "auto");
  await workers.selectOption("2");
  await page.waitForFunction(
    (select) => select.value === "2",
    await workers.elementHandle(),
  );
  await button(settingsPanel, "Select Folder…").click();
  await blocked.waitFor();
  assert.equal(bridge.callsFor("select_inventory_folder").length, 1);
  assert.equal(bridge.callsFor("use_automatic_inventory").length, 0);
  await button(settingsPanel, "Use auto-detect").click();
  await button(settingsPanel, "Refresh").waitFor();
  assert.equal(bridge.callsFor("refresh_inventory").length, 1);
  assert.equal(bridge.callsFor("use_automatic_inventory").length, 1);
  await button(settingsPanel, "Back to editor").click();
  assert.equal(await goldField(page).inputValue(), "002");
  await button(page, "Settings").click();
  assert.equal(await workers.inputValue(), "2");
  await workers.selectOption("auto");
  await page.waitForFunction(
    (select) => select.value === "auto",
    await workers.elementHandle(),
  );
  assert.deepEqual(bridge.callsFor("change_setting"), [
    { change: { key: "mapIndexWorkers", value: "2" } },
    { change: { key: "mapIndexWorkers", value: "auto" } },
  ]);
  await button(settingsPanel, "Back to editor").click();
  assert.equal(await goldField(page).inputValue(), "002");
  await button(page, "Apply edits").click();
}

async function verifyToolsMenu(page) {
  const tools = button(page, "Tools");
  await tools.press("Enter");
  const menu = page.getByRole("menu");
  await menu
    .getByRole("menuitem", { name: "Re-sign external bank…", exact: true })
    .waitFor();
  await menu
    .getByRole("menuitem", { name: "Back up all bank files", exact: true })
    .waitFor();
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });
  await tools.and(page.locator(":focus")).waitFor();
}

async function verifyBackupCopy(page, bridge, canOverwrite) {
  const inventoryStatusCalls = bridge.callsFor("inventory_backup_status");
  assert(inventoryStatusCalls.length > 0);
  for (const call of inventoryStatusCalls) {
    assert.deepEqual(call, { generation: inventory.generation });
  }
  await button(page, "More save options").click();
  await page
    .getByRole("menuitem", {
      name: canOverwrite ? "Restore Backup…" : "Bank Backups…",
      exact: true,
    })
    .click();
  const dialog = page.getByRole("dialog", {
    name: canOverwrite ? "Restore Bank Backup" : "Bank Backups",
    exact: true,
  });
  assert(
    await dialog
      .getByRole("radio", { name: /Unavailable\.SC2Bank/ })
      .isDisabled(),
  );
  assert(
    await dialog.getByRole("radio", { name: /Verified\.SC2Bank/ }).isChecked(),
  );
  assert.equal(
    await button(dialog, "Review Restore…").count(),
    canOverwrite ? 1 : 0,
  );
  await button(dialog, "Save a Copy…").click();
  await page
    .getByRole("status")
    .filter({ hasText: "Saved backup copy Backup Copy.SC2Bank." })
    .waitFor();

  assert(await button(page, "Undo").isEnabled());
  assert.equal(await goldField(page).inputValue(), "002");
  assert.deepEqual(
    bridge.callsFor("inventory_backup_status"),
    inventoryStatusCalls,
  );
  const lists = bridge.callsFor("list_active_backups");
  assert.equal(lists.length, 1);
  assert.deepEqual(bridge.callsFor("export_active_backup"), [
    { target: lists[0].target, backupId: verifiedBackupId },
  ]);
  assert.deepEqual(bridge.callsFor("restore_active_bank"), []);
  assert.deepEqual(bridge.callsFor("save_active_bank"), []);
  assert.deepEqual(bridge.callsFor("save_bank_as"), []);
  assert.equal(bridge.callsFor("open_bank").length, 1);
}

async function verifyBackupRestoreReview(page, bridge) {
  await button(page, "More save options").click();
  await page.getByRole("menuitem", { name: "Restore Backup…" }).click();
  const dialog = page.getByRole("dialog", { name: "Restore Bank Backup" });
  await button(dialog, "Review Restore…").click();
  await dialog.getByRole("heading", { name: "Current file" }).waitFor();
  await dialog.getByRole("heading", { name: "Backup", exact: true }).waitFor();
  assert.deepEqual(await dialog.locator("pre").allTextContents(), [
    initialBank.toString(),
    initialBank.toString().replace('"001"', '"003"'),
  ]);
  await dialog.getByText(/discards the unsaved changes/).waitFor();
  assert.deepEqual(bridge.callsFor("restore_active_bank"), []);
  await button(dialog, "Back").click();
  await button(dialog, "Review Restore…").click();
  await button(dialog, "Restore…").click();
  await dialog.waitFor({ state: "hidden" });
  const reviewed = {
    target: bridge.callsFor("list_active_backups").at(-1).target,
    backupId: verifiedBackupId,
  };
  assert.deepEqual(bridge.callsFor("review_active_backup"), [
    reviewed,
    reviewed,
  ]);
  assert.deepEqual(bridge.callsFor("restore_active_bank"), [reviewed]);
  assert.equal(await goldField(page).inputValue(), "002");
}

async function verifyBankNavigation(page, bridge, canOverwrite) {
  const open = button(page, "Open Bank…");
  const saveCommand = canOverwrite ? "save_active_bank" : "save_bank_as";
  const otherSaveCommand = canOverwrite ? "save_bank_as" : "save_active_bank";
  async function choose(choice, outcome) {
    bridge.choose(choice, outcome);
    const count = bridge.callsFor("confirm_bank_navigation").length;
    await open.click();
    await page.waitForFunction(
      (expected) => window.fixtureNavigationReplies === expected,
      count + 1,
    );
    await open.click({ trial: true });
  }

  for (const [choice, outcome, savedAttempts] of [
    ["cancel", undefined, 0],
    ["save", "cancel", 1],
    ["save", "fail", 2],
  ]) {
    await choose(choice, outcome);
    if (outcome === "fail") {
      await page
        .getByRole("status")
        .filter({ hasText: storageRefusal })
        .waitFor();
    }
    assert.equal(bridge.callsFor(saveCommand).length, savedAttempts);
    assert.equal(bridge.callsFor("open_bank").length, 1);
    assert.equal(await goldField(page).inputValue(), "002");
    assert(await button(page, "Undo").isEnabled());
  }

  await choose("save", "success");
  await page.getByRole("heading", { name: "Replacement-2.SC2Bank" }).waitFor();
  assert.equal(await goldField(page).inputValue(), "001");
  const saves = bridge.callsFor(saveCommand);
  assert.equal(saves.length, 3);
  assert.deepEqual(bridge.callsFor(otherSaveCommand), []);
  assert(
    saves.every(
      ({ withBackup }) => withBackup === (canOverwrite ? false : undefined),
    ),
  );
  const opens = bridge.callsFor("open_bank");
  assert.equal(opens.length, 2);
  assert.equal(
    JSON.parse(opens[1].control).command.payload?.discard_current,
    undefined,
  );

  await editGold(page, "003");
  await choose("discard");
  await page.getByRole("heading", { name: "Replacement-3.SC2Bank" }).waitFor();
  assert.equal(await goldField(page).inputValue(), "001");
  assert.equal(bridge.callsFor(saveCommand).length, 3);
  const discarded = JSON.parse(bridge.callsFor("open_bank").at(-1).control);
  const approval = bridge.callsFor("confirm_bank_navigation").at(-1);
  assert.equal(approval.canSave, true);
  assert.equal(discarded.request_id, approval.target.requestId);
  assert.deepEqual(discarded.command, {
    kind: "open",
    payload: { discard_current: true },
  });
}

async function finishAcceptedOpen(page, bridge, expectedName, value = "001") {
  const finish = button(page, "Finish opening bank");
  const lastRead = bridge.wireRequests.at(-1);
  assert.equal(lastRead.command.kind, "raw_text");
  const before = bridge.wireRequests.length;
  const opens = bridge.callsFor("open_bank");
  await finish.click();
  await finish.waitFor({ state: "hidden" });
  await page
    .getByRole("heading", { name: expectedName, exact: true })
    .waitFor();
  const reads = bridge.wireRequests.slice(before);
  assert.deepEqual(
    reads.map(({ command }) => command.kind),
    ["snapshot", "raw_text"],
  );
  for (const read of reads) {
    assert.equal(read.document_id, lastRead.document_id);
    assert.equal(read.expected_revision, lastRead.expected_revision);
  }
  assert.deepEqual(bridge.callsFor("open_bank"), opens);
  assert.equal(await goldField(page).inputValue(), value);
}

async function verifyAcceptedOpen(page, bridge, canOverwrite) {
  bridge.failNextOpenProjection();
  const open = button(page, "Open Bank…");
  const finish = button(page, "Finish opening bank");
  await open.click();
  await finish.waitFor();
  assert(await open.isDisabled());
  assert.equal(await goldField(page).count(), 0);
  await finishAcceptedOpen(page, bridge, initialName);
  assert(await open.isEnabled());

  await editGold(page, "002");
  bridge.choose("discard");
  bridge.failNextOpenProjection();
  await open.click();
  await finish.waitFor();
  assert(await page.getByRole("heading", { name: initialName }).isVisible());
  await page.getByRole("cell", { name: "002", exact: true }).waitFor();
  assert.equal(await goldField(page).count(), 0);
  assert(await button(page, canOverwrite ? "Save" : "Save As…").isDisabled());
  assert(await open.isDisabled());
  await finishAcceptedOpen(page, bridge, "Replacement-2.SC2Bank");
  assert.deepEqual(bridge.callsFor("save_active_bank"), []);
  assert.deepEqual(bridge.callsFor("save_bank_as"), []);
}

async function verifyAcceptedConflictReopen(page, bridge, name) {
  await editGold(page, "004");
  const opens = bridge.callsFor("open_bank");
  bridge.choose("save", "external");
  bridge.failNextOpenProjection();
  await button(page, "Open Bank…").click();
  const conflict = page.getByRole("dialog", { name: "Bank changed on disk" });
  await button(conflict, "Compare").click();
  await button(conflict, "Reopen from Disk…").click();
  await conflict.waitFor({ state: "hidden" });
  await button(page, "Finish opening bank").waitFor();
  await page.getByRole("cell", { name: "004", exact: true }).waitFor();
  assert(await button(page, "Open Bank…").isDisabled());
  assert(await button(page, "Save").isDisabled());
  assert.deepEqual(bridge.callsFor("open_bank"), opens);
  const reopens = bridge.callsFor("reopen_bank_conflict");
  assert.equal(reopens.length, 1);
  const saves = bridge.callsFor("save_active_bank");
  assert.equal(saves.length, 1);
  await finishAcceptedOpen(page, bridge, name, "006");
  assert.deepEqual(bridge.callsFor("reopen_bank_conflict"), reopens);
  assert.deepEqual(bridge.callsFor("save_active_bank"), saves);
  assert.deepEqual(bridge.callsFor("open_bank"), opens);
}

async function verifyAcceptedBackupRestore(page, bridge, name) {
  await editGold(page, "009");
  bridge.acceptNextRestore();
  bridge.failNextOpenProjection();
  await button(page, "More save options").click();
  await page.getByRole("menuitem", { name: "Restore Backup…" }).click();
  const restore = page.getByRole("dialog", { name: "Restore Bank Backup" });
  await button(restore, "Review Restore…").click();
  await button(restore, "Restore…").click();
  await restore.waitFor({ state: "hidden" });
  await button(page, "Finish opening bank").waitFor();
  await page
    .getByRole("status")
    .filter({
      hasText: `Restored ${name} on disk and saved a reversal backup.`,
    })
    .waitFor();
  await page.getByRole("cell", { name: "009", exact: true }).waitFor();
  assert(await button(page, "Save").isDisabled());
  const restores = bridge.callsFor("restore_active_bank");
  assert.equal(restores.length, 1);
  const saves = bridge.callsFor("save_active_bank");
  await finishAcceptedOpen(page, bridge, name, "003");
  assert.deepEqual(bridge.callsFor("restore_active_bank"), restores);
  assert.deepEqual(bridge.callsFor("save_active_bank"), saves);
}

async function verifyWriteRecovery(page, bridge) {
  const destination = {
    display: "/synthetic/banks/Recovery Fixture.SC2Bank",
    truncated: false,
    lossy: false,
  };
  const recoveryPath = {
    display: "/synthetic/banks/Recovery\uFFFD\r\nFixture.pending",
    truncated: true,
    lossy: true,
  };
  const backupPath = {
    ...destination,
    display: "/synthetic/backups/Recovery Fixture.SC2Bank.bak",
  };
  const uncertain = {
    operation: "save_overwrite",
    outcome: "commit_uncertain",
    affectsActive: true,
    destination,
    recovery: { kind: "recorded", path: recoveryPath },
    backup: { kind: "verified", path: backupPath },
    reason: "Synthetic write verification failure.",
    reasonTruncated: true,
  };
  const dialog = page.getByRole("dialog", {
    name: "Write recovery details",
    exact: true,
  });
  const activeSave = button(page, "Save");
  const saveCopy = button(page, "Save As…");
  const dirty = page
    .getByRole("status")
    .filter({ hasText: "Unsaved changes in 1 section" });
  const tools = button(page, "Tools");
  const review = page.getByRole("menuitem", {
    name: "Review write recovery…",
    exact: true,
  });
  const activeWritesBefore = bridge.callsFor("save_active_bank").length;

  async function closeDetails() {
    await button(dialog, "Close").click();
    await dialog.waitFor({ state: "hidden" });
  }

  async function failWrite(command, receipt, trigger) {
    const before = bridge.callsFor(command).length;
    const current = bridge.wireRequests.findLast(
      ({ command }) => command.kind === "raw_text",
    );
    const wireCount = bridge.wireRequests.length;
    bridge.failNextWrite(command, receipt);
    await trigger();
    await dialog.waitFor();
    const calls = bridge.callsFor(command);
    assert.equal(calls.length, before + 1);
    const request = calls.at(-1);
    const target = request.target
      ? {
          document_id: request.target.documentId,
          expected_revision: request.target.revision,
        }
      : JSON.parse(request.control);
    assert.equal(target.document_id, current.document_id);
    assert.equal(target.expected_revision, current.expected_revision);
    assert.equal(bridge.wireRequests.length, wireCount);
  }

  async function checkPath(name, path) {
    const field = dialog.getByRole("textbox", { name, exact: true });
    assert.equal(await field.evaluate((input) => input.readOnly), true);
    assert.equal(
      await field.inputValue(),
      path.display.replaceAll("\r\n", "\n"),
    );
    const copied = await field.evaluate((input) => {
      input.select();
      const clipboardData = new DataTransfer();
      const event = new ClipboardEvent("copy", {
        bubbles: true,
        cancelable: true,
        clipboardData,
      });
      input.dispatchEvent(event);
      return {
        prevented: event.defaultPrevented,
        text: clipboardData.getData("text/plain"),
      };
    });
    assert.deepEqual(copied, { prevented: true, text: path.display });
  }

  await editGold(page, "009");
  await failWrite(
    "save_active_bank",
    {
      ...uncertain,
      outcome: "live_unchanged",
      affectsActive: false,
      backup: { kind: "unverified", path: backupPath },
    },
    () => activeSave.click(),
  );
  await dialog.getByText(/destination was left unchanged/i).waitFor();
  await checkPath("Unverified backup location", backupPath);
  assert.equal(
    await dialog
      .getByRole("textbox", { name: "Verified backup location", exact: true })
      .count(),
    0,
  );
  await closeDetails();
  assert.equal(await activeSave.isEnabled(), true);
  assert.equal(await goldField(page).inputValue(), "009");
  await dirty.waitFor();

  await failWrite(
    "export_bank_copy",
    {
      ...uncertain,
      operation: "export_copy",
      affectsActive: false,
      recovery: { kind: "unknown" },
      backup: { kind: "not_created" },
    },
    async () => {
      await button(page, "More save options").click();
      await page
        .getByRole("menuitem", { name: "Export copy…", exact: true })
        .click();
    },
  );
  await dialog.getByText("Export copy", { exact: false }).waitFor();
  await dialog
    .getByText(/source bank was left unchanged|open bank was left unchanged/i)
    .waitFor();
  await dialog
    .getByText("No recovery location was recorded.", { exact: true })
    .waitFor();
  await dialog
    .getByText("No backup was created during the operation.", { exact: true })
    .waitFor();
  await closeDetails();
  assert.equal(await activeSave.isEnabled(), true);
  assert.equal(await goldField(page).inputValue(), "009");
  await dirty.waitFor();

  await failWrite("save_active_bank", uncertain, () => activeSave.click());
  await dialog
    .getByText(/could not verify this write.*before trying to overwrite it/i)
    .waitFor();
  await dialog.getByText(/recorded during the operation/i).waitFor();
  await checkPath("Destination path", destination);
  await checkPath("Recorded recovery location", recoveryPath);
  await checkPath("Verified backup location", backupPath);
  await dialog
    .getByText("The recorded path was shortened.", { exact: true })
    .waitFor();
  await dialog
    .getByText("Some path characters could not be represented.", {
      exact: true,
    })
    .waitFor();
  await dialog
    .getByText("The reported reason was shortened.", { exact: true })
    .waitFor();
  await dialog.getByText(uncertain.reason, { exact: true }).waitFor();
  await closeDetails();
  assert.equal(await activeSave.count(), 0);
  assert.equal(await saveCopy.isEnabled(), true);
  assert.equal(await goldField(page).inputValue(), "009");
  await dirty.waitFor();

  await tools.click();
  await review.click();
  await dialog.waitFor();
  await checkPath("Recorded recovery location", recoveryPath);
  await closeDetails();
  assert.equal(
    bridge.callsFor("save_active_bank").length,
    activeWritesBefore + 2,
  );

  bridge.choose("discard");
  const nextOpen = bridge.callsFor("open_bank").length + 1;
  await button(page, "Open Bank…").click();
  await page
    .getByRole("heading", {
      name: `Replacement-${nextOpen}.SC2Bank`,
      exact: true,
    })
    .waitFor();
  assert.equal(await goldField(page).inputValue(), "001");
  assert.equal(await activeSave.isEnabled(), true);
  await tools.click();
  await page.getByRole("menu").waitFor();
  assert.equal(await review.count(), 0);
  await page.keyboard.press("Escape");

  await editGold(page, "010");
  const untrusted = "Synthetic malformed receipt path and reason";
  await failWrite(
    "save_active_bank",
    { ...uncertain, affectsActive: false, reason: untrusted },
    () => activeSave.click(),
  );
  await dialog
    .getByText(/could not read the write recovery receipt/i)
    .waitFor();
  assert.equal(await dialog.getByRole("textbox").count(), 0);
  assert.equal(await dialog.getByText(untrusted, { exact: true }).count(), 0);
  await closeDetails();
  assert.equal(await activeSave.count(), 0);
  assert.equal(await saveCopy.isEnabled(), true);
  assert.equal(await goldField(page).inputValue(), "010");
  await dirty.waitFor();
  assert.equal(
    bridge.callsFor("save_active_bank").length,
    activeWritesBefore + 3,
  );
}

async function verifyMalformedBackupList(page, bridge) {
  await editGold(page, "004");
  bridge.corruptBackupList();
  await button(page, "More save options").click();
  await page.getByRole("menuitem", { name: "Restore Backup…" }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "StarBank received an invalid desktop response." })
    .waitFor();
  assert.equal(await goldField(page).inputValue(), "004");
  assert.equal(
    await page.getByRole("dialog", { name: "Restore Bank Backup" }).count(),
    0,
  );
}

async function verifyDraftSave(page, bridge, canOverwrite) {
  const saveCommand = canOverwrite ? "save_active_bank" : "save_bank_as";
  const before = bridge.callsFor(saveCommand).length;
  bridge.choose("save", "success");
  await goldField(page).fill("9,007,199,254,740,993");
  await page.evaluate(() => {
    const editor = document.activeElement;
    for (let index = 0; index < 2; index += 1) {
      editor.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "s",
          ctrlKey: true,
          bubbles: true,
        }),
      );
    }
  });
  await page
    .getByRole("status")
    .filter({ hasText: /Saved .*\.SC2Bank\./ })
    .waitFor();
  assert.equal(bridge.callsFor(saveCommand).length, before + 1);
  const raw = page.getByRole("textbox", { name: "Bank XML" });
  await page.getByRole("tab", { name: "Raw View" }).click();
  await raw.filter({ hasText: 'int="9007199254740993"' }).waitFor();

  await page.getByRole("tab", { name: "Table View" }).click();
  const gold = goldField(page);
  await gold.focus();
  await gold.fill("0042");
  assert.equal(await gold.inputValue(), "0042");
  bridge.supersedeNextEdit();
  await gold.press("Control+s");
  await page
    .getByRole("status")
    .filter({
      hasText: "Refresh the editor to check the current bank revision.",
    })
    .waitFor();
  assert.equal(await gold.inputValue(), "0042");
  assert(await button(page, "Discard draft").isVisible());
  assert.equal(bridge.callsFor(saveCommand).length, before + 1);
  const primarySave = button(page, canOverwrite ? "Save" : "Save As…");
  assert(await primarySave.isDisabled());
  await refreshEditorOnly(page, bridge);
  assert(
    await primarySave.isDisabled(),
    "a draft from an older revision stays blocked",
  );
  await page.getByText("Review Table draft", { exact: true }).click();
  const reviewed = page.getByRole("textbox", { name: "Original Table draft" });
  assert.deepEqual(JSON.parse(await reviewed.inputValue()), [
    { section: "Player", field: "Gold", value: "0042" },
  ]);
  await button(page, "Discard draft").click();
  assert.equal(await goldField(page).inputValue(), "001");

  await editGold(page, "1000");
  await goldField(page).focus();
  assert.equal(await goldField(page).inputValue(), "1000");
  await goldField(page).fill("1,000");
  await button(page, "Discard draft").waitFor();
  const editCount = bridge.wireRequests.filter(
    ({ command }) => command.kind === "set_values",
  ).length;
  bridge.failNextEditProjection();
  await goldField(page).press("Control+s");
  await page
    .getByRole("status")
    .filter({
      hasText: "Refresh the editor to check the current bank revision.",
    })
    .waitFor();
  assert(await primarySave.isDisabled());
  await refreshEditorOnly(page, bridge);
  assert(
    await primarySave.isEnabled(),
    "the draft still belongs to the same no-op revision",
  );
  assert(await button(page, "Discard draft").isVisible());
  assert.equal(await goldField(page).inputValue(), "1,000");
  assert.equal(
    bridge.wireRequests.filter(({ command }) => command.kind === "set_values")
      .length,
    editCount + 1,
  );
  assert.equal(bridge.callsFor(saveCommand).length, before + 1);
  await button(page, "Discard draft").click();
}

async function refreshEditorOnly(page, bridge) {
  const before = bridge.wireRequests.length;
  await button(page, "Refresh editor").click();
  await page
    .getByRole("status")
    .filter({ hasText: "Editor refreshed." })
    .waitFor();
  const reads = bridge.wireRequests
    .slice(before)
    .filter(({ command }) => command.kind !== "validate_values");
  assert.deepEqual(
    reads.map(({ command }) => command.kind),
    ["snapshot", "raw_text"],
  );
  assert.equal(reads[0].document_id, reads[1].document_id);
  assert.equal(reads[0].expected_revision, reads[1].expected_revision);
}

async function verifyCreateOnlyCopyActions(page, bridge, toolsMenu, external) {
  await external.press("Enter");
  await page
    .getByRole("status")
    .filter({ hasText: `Saved verified signed copy ${signedCopyName}.` })
    .waitFor();
  await toolsMenu.waitFor({ state: "hidden" });
  assert.deepEqual(bridge.callsFor("pick_external_bank_to_resign"), [{}]);
  assert.deepEqual(bridge.callsFor("export_external_signed_bank"), [
    { id: externalSigningSourceId, context: null },
  ]);
  assert.deepEqual(bridge.callsFor("discard_external_signing_source"), [
    { id: externalSigningSourceId },
  ]);

  const dirty = page.getByRole("status").filter({ hasText: "Unsaved changes" });
  await dirty.waitFor();
  const sourceRevision = bridge.wireRequests.findLast(
    ({ command }) => command.kind === "raw_text",
  );
  await button(page, "More save options").click();
  await page
    .getByRole("menuitem", { name: "Export copy…", exact: true })
    .click();
  await page
    .getByRole("status")
    .filter({ hasText: `Exported ${exportedCopyName}.` })
    .waitFor();
  const exports = bridge.callsFor("export_bank_copy");
  assert.equal(exports.length, 1);
  const [{ control, suggestedName }] = exports;
  const request = JSON.parse(control);
  assert.equal(suggestedName, initialName);
  assert.deepEqual(request.command, { kind: "prepare" });
  assert.equal(request.document_id, sourceRevision.document_id);
  assert.equal(request.expected_revision, sourceRevision.expected_revision);
  assert(await dirty.isVisible());
  assert.equal(await goldField(page).inputValue(), "002");
  assert(await page.getByRole("heading", { name: initialName }).isVisible());

  bridge.choose("save", "success");
  const saveAs = button(page, "Save As…");
  await saveAs.click();
  await page
    .getByRole("heading", { name: savedCopyName, exact: true })
    .waitFor();
  await dirty.waitFor({ state: "hidden" });
  assert.equal(bridge.callsFor("save_bank_as").length, 1);

  const gold = goldField(page);
  await gold.fill("9,007,199,254,740,993");
  await gold.press("Control+s");
  await page.waitForFunction(() => window.fixtureSaveAsReplies === 2);
  await saveAs.click({ trial: true });
  const saves = bridge.callsFor("save_bank_as");
  assert.equal(saves.length, 2);
  assert.notEqual(saves[0].target.revision, saves[1].target.revision);
  await page.getByRole("tab", { name: "Raw View" }).click();
  await page
    .getByRole("textbox", { name: "Bank XML" })
    .filter({ hasText: 'int="9007199254740993"' })
    .waitFor();
}

async function verifyCreateOnlyRenderer(page, bridge, canCreateCopy) {
  await page.waitForFunction(
    () => window.fixtureCopyCapabilityReplied === true,
  );
  await openBank(page);
  await editGold(page, "002");

  await button(page, "Settings").click();
  const settingsPanel = page.getByRole("region", { name: "Settings" });
  const platform = settingsPanel
    .getByRole("term")
    .filter({ hasText: /^Platform$/u });
  assert.equal(
    await platform.locator("..").getByRole("definition").textContent(),
    buildPlatformLabel,
  );
  assert.equal(await settingsPanel.getByLabel("Automatic backup").count(), 0);
  assert.equal(
    await settingsPanel.getByText("Managed backup folder").count(),
    0,
  );
  if (buildPlatformLabel === "Windows") {
    await settingsPanel
      .getByText(
        canCreateCopy
          ? "Save As and file export require an unused filename on a local NTFS volume. Overwrite is unavailable in this Windows build."
          : "Bank editing is available in memory. Disk save and file export are unavailable in this Windows build.",
        { exact: true },
      )
      .waitFor();
  }
  await button(settingsPanel, "Back to editor").click();

  const open = button(page, "Open Bank…");
  bridge.choose("save", "success");
  await open.click();
  await page.waitForFunction(() => window.fixtureNavigationReplies === 1);
  await open.click({ trial: true });
  assert.equal(bridge.callsFor("confirm_bank_navigation")[0].canSave, false);
  assert.equal(bridge.callsFor("open_bank").length, 1);

  bridge.chooseClose("save");
  await page.waitForFunction(
    () => typeof window.fixtureCloseProbe === "function",
  );
  await page.evaluate(() => window.fixtureCloseProbe({ ticket: 1 }));
  await open.click({ trial: true });
  const [{ report }] = bridge.callsFor("confirm_window_close");
  assert.equal(report.canSave, false);
  assert.equal(report.hasDraft, false);
  assert.deepEqual(bridge.callsFor("complete_window_close"), [
    { ticket: 1, draftGeneration: report.draftGeneration, proceed: false },
  ]);
  assert.deepEqual(bridge.callsFor("save_bank_as"), []);
  assert.equal(await goldField(page).inputValue(), "002");

  assert.equal(await button(page, "Save").count(), 0);
  assert.equal(await button(page, "Save As…").count(), Number(canCreateCopy));
  await button(page, "More save options").click();
  const menu = page.getByRole("menu");
  assert.equal(
    await menu
      .getByRole("menuitem", {
        name: /^(Save \(Overwrite\)|Save w\/ Backup|Restore Backup…|Bank Backups…)$/u,
      })
      .count(),
    0,
  );
  assert.equal(
    await menu
      .getByRole("menuitem", { name: "Export copy…", exact: true })
      .count(),
    Number(canCreateCopy),
  );
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });

  await button(page, "Tools").press("Enter");
  const toolsMenu = page.getByRole("menu");
  const external = toolsMenu.getByRole("menuitem", {
    name: "Re-sign external bank…",
    exact: true,
  });
  await external.waitFor();
  assert.equal(await external.isEnabled(), canCreateCopy);
  for (const name of ["Back up all bank files", "Back up selected map"]) {
    assert(
      await toolsMenu.getByRole("menuitem", { name, exact: true }).isDisabled(),
    );
  }
  if (canCreateCopy) {
    await verifyCreateOnlyCopyActions(page, bridge, toolsMenu, external);
  } else {
    await page.keyboard.press("Escape");
    await goldField(page).press("Control+s");
    for (const command of [
      "save_bank_as",
      "export_bank_copy",
      "pick_external_bank_to_resign",
      "export_external_signed_bank",
    ]) {
      assert.deepEqual(bridge.callsFor(command), []);
    }
  }

  for (const command of [
    "save_active_bank",
    "list_active_backups",
    "restore_active_bank",
    "review_active_backup",
    "export_active_backup",
    "create_batch_backup",
    "read_backup_preferences",
    "choose_backup_location",
    "set_backup_retention",
  ]) {
    assert.deepEqual(bridge.callsFor(command), []);
  }
}

async function verifyRenderer(
  browser,
  canOverwrite,
  openRecovery = false,
  copyCapability,
) {
  const context = await browser.newContext({
    offline: true,
    serviceWorkers: "block",
    viewport: { width: 850, height: 500 },
    colorScheme: canOverwrite ? "dark" : "light",
  });
  const engine = new WasmBankEngine();
  try {
    const rejected = [];
    await context.route("**/*", async (route) => {
      const request = route.request();
      const url = new URL(request.url());
      const asset = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
      const file = resolve(dist, asset);
      if (
        request.method() !== "GET" ||
        url.origin !== origin ||
        !file.startsWith(dist + sep)
      ) {
        rejected.push(request.url());
        await route.abort();
        return;
      }
      await route.fulfill({ path: file });
    });

    const page = await context.newPage();
    page.setDefaultTimeout(10_000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    const bridge = createBridge(engine, canOverwrite, copyCapability);
    await bindRenderer(page, bridge);
    await page.goto(origin);
    if (copyCapability !== undefined) {
      await verifyCreateOnlyRenderer(
        page,
        bridge,
        copyCapability.value === true,
      );
      assert.deepEqual(bridge.unexpected, []);
      assert.deepEqual(errors, []);
      assert.deepEqual(rejected, []);
      console.log(
        `Chromium ${browser.version()} ${buildPlatformLabel} desktop bundle verified independent create-only capability (${JSON.stringify(copyCapability)}).`,
      );
      return;
    }
    if (openRecovery === "close_probe") {
      await verifyEmptyEngineCloseProbe(page, bridge);
      assert.deepEqual(bridge.unexpected, []);
      assert.deepEqual(errors, []);
      assert.deepEqual(rejected, []);
      console.log(
        `Chromium ${browser.version()} desktop renderer passed empty-engine close probes.`,
      );
      return;
    }
    if (openRecovery === "verified_write") {
      await verifyVerifiedNeedsReopen(page, bridge);
      assert.deepEqual(bridge.unexpected, []);
      assert.deepEqual(errors, []);
      assert.deepEqual(rejected, []);
      console.log(
        `Chromium ${browser.version()} desktop renderer passed verified-write reopening.`,
      );
      return;
    }
    if (openRecovery === "structure_close") {
      await openBank(page);
      await verifyNativeStructureDraftClose(page, bridge);
      assert.deepEqual(bridge.unexpected, []);
      assert.deepEqual(errors, []);
      assert.deepEqual(rejected, []);
      console.log(
        `Chromium ${browser.version()} desktop renderer guarded the unsubmitted structure draft during close.`,
      );
      return;
    }
    if (openRecovery) {
      await verifyAcceptedOpen(page, bridge, canOverwrite);
      const currentName = "Replacement-2.SC2Bank";
      await verifyAcceptedConflictReopen(page, bridge, currentName);
      await verifyAcceptedBackupRestore(page, bridge, currentName);
      await verifyWriteRecovery(page, bridge);
      assert.deepEqual(bridge.unexpected, []);
      assert.deepEqual(errors, []);
      assert.deepEqual(rejected, []);
      console.log(
        `Chromium ${browser.version()} desktop renderer passed accepted-open recovery and write recovery receipts.`,
      );
      return;
    }
    if (canOverwrite) {
      await verifyInventoryRecovery(page, bridge);
      await verifyBatchBackupReview(page);
    }
    await button(
      page,
      /Synthetic Map.*1 bank without a recorded backup/,
    ).waitFor();
    const firstMap = await button(page, /Synthetic Map/).boundingBox();
    const firstBank = await button(page, /Synthetic\.SC2Bank/).boundingBox();
    const options = await page.locator(".map-options").boundingBox();
    assert(firstMap && firstBank && options);
    assert(firstMap.y + firstMap.height <= firstBank.y);
    assert(firstBank.y < options.y);
    if (!canOverwrite) await openBank(page);
    const footer = await page.locator(".editor-footer").boundingBox();
    assert(footer && footer.y + footer.height < 500);
    if (canOverwrite) await verifyToolsMenu(page);
    else await editGold(page, "002");
    await verifyRevert(page);
    await verifyBackupCopy(page, bridge, canOverwrite);
    if (canOverwrite) await verifyBackupRestoreReview(page, bridge);
    await verifyBankNavigation(page, bridge, canOverwrite);
    if (canOverwrite) await verifyMalformedBackupList(page, bridge);
    await verifyDraftSave(page, bridge, canOverwrite);
    assert.deepEqual(bridge.unexpected, []);
    assert.deepEqual(errors, []);
    assert.deepEqual(rejected, []);
    console.log(
      `Chromium ${browser.version()} desktop renderer passed dirty tracking, save checkpoint revert, backup copy and guarded navigation (${canOverwrite ? "overwrite" : "save as"}).`,
    );
  } finally {
    await context.close();
    engine.free();
  }
}

await init({ module_or_path: await readFile(wasm) });
const browser = await chromium.launch({ headless: true, timeout: 15_000 });
try {
  await verifyRenderer(browser, true);
  await verifyRenderer(browser, false);
  await verifyRenderer(browser, true, true);
  await verifyRenderer(browser, true, "verified_write");
  await verifyRenderer(browser, true, "close_probe");
  await verifyRenderer(browser, true, "structure_close");
  for (const capability of [
    { value: true },
    { value: false },
    { value: "true" },
    { error: "Synthetic create-copy capability refusal." },
  ]) {
    await verifyRenderer(browser, false, undefined, capability);
  }
} finally {
  await browser.close();
}
