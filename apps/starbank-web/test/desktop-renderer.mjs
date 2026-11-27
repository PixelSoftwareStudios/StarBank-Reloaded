import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import init, { WasmBankEngine } from "../src/generated/starbank_wasm.js";

const dist = fileURLToPath(
  new URL("../../starbank-desktop/dist", import.meta.url),
);
const origin = "https://starbank-desktop.test";
const wasm = new URL("../src/generated/starbank_wasm_bg.wasm", import.meta.url);
const initialName = "Synthetic.SC2Bank";
const initialBank = Buffer.from(
  '<Bank version="1"><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section></Bank>',
);
const verifiedBackupId = 1;
const storageRefusal = "Synthetic storage refusal.";
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

function dispatchFrame(engine, packet) {
  const frame = Buffer.from(packet);
  const controlEnd = frameHeaderBytes + frame.readUInt32LE(controlLengthOffset);
  assert(controlEnd <= frame.length);
  const attached = frame[0] === 1;
  assert(attached || (frame[0] === 0 && controlEnd === frame.length));
  const reply = dispatch(
    engine,
    frame.subarray(frameHeaderBytes, controlEnd).toString(),
    attached ? frame.subarray(controlEnd) : undefined,
  );

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

function createBridge(engine, canOverwrite) {
  const calls = [];
  const unexpected = [];
  let navigation;
  let openCount = 0;
  let saved = false;
  let malformedBackupList = false;

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

  function invoke(command, args) {
    if (command === "dispatch_bank")
      return { value: dispatchFrame(engine, args) };
    calls.push({ command, args });
    switch (command) {
      case "read_settings":
        return {
          value: {
            settings,
            legacyRootPending: false,
            automaticBackupAvailable: canOverwrite,
          },
        };
      case "read_backup_preferences":
        return { value: { location: { mode: "default" }, keepNewest: null } };
      case "list_open_requests":
        return { value: [] };
      case "can_export_bank":
        return { value: true };
      case "can_reveal_bank":
        return { value: false };
      case "refresh_inventory":
        return { value: inventory };
      case "inventory_backup_status":
        return { value: [{ bankId: "bank-a", state: "never_backed_up" }] };
      case "plugin:event|listen":
        return { value: args.handler };
      case "plugin:event|unlisten":
        return {};
      case "open_bank": {
        const request = JSON.parse(args.control);
        // The WASM wire protocol leaves native save acknowledgement to the host.
        // After a synthetic acknowledgement, let WASM replace the saved session.
        if (saved) {
          assert.equal(request.command.payload?.discard_current, undefined);
          request.command.payload = { discard_current: true };
          saved = false;
        }
        const reply = dispatch(engine, JSON.stringify(request), initialBank);
        openCount += 1;
        return {
          value: {
            name:
              openCount === 1
                ? initialName
                : `Replacement-${openCount}.SC2Bank`,
            can_overwrite: canOverwrite,
            control: reply.control,
          },
        };
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
        dirtySnapshot(args.target);
        return { value: null };
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
        assert.equal(navigation.outcome, "success");
        snapshot.result.payload.dirty = false;
        saved = true;
        return {
          value: {
            name: canOverwrite ? initialName : "Saved Copy.SC2Bank",
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
    window.fixtureNavigationReplies = 0;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => ++callbackId,
      unregisterCallback() {},
      async invoke(command, args) {
        const binary = command === "dispatch_bank";
        const reply = await window.invokeDesktopFixture(
          command,
          binary ? Array.from(args) : args,
        );
        if (command === "confirm_bank_navigation")
          window.fixtureNavigationReplies += 1;
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
  assert.deepEqual(bridge.callsFor("inventory_backup_status"), [
    { generation: inventory.generation },
  ]);
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

async function verifyRenderer(browser, canOverwrite) {
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
    const bridge = createBridge(engine, canOverwrite);
    await bindRenderer(page, bridge);
    await page.goto(origin);
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
    await button(page, "Open Bank…").click();
    await page
      .getByRole("heading", { name: initialName, exact: true })
      .waitFor();
    const footer = await page.locator(".editor-footer").boundingBox();
    assert(footer && footer.y + footer.height < 500);
    if (canOverwrite) await verifyToolsMenu(page);
    await editGold(page, "002");
    await verifyRevert(page);
    await verifyBackupCopy(page, bridge, canOverwrite);
    if (canOverwrite) await verifyBackupRestoreReview(page, bridge);
    await verifyBankNavigation(page, bridge, canOverwrite);
    if (canOverwrite) await verifyMalformedBackupList(page, bridge);
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
} finally {
  await browser.close();
}
