import assert from "node:assert/strict";
import { basename, isAbsolute } from "node:path";

const settings = 'section[aria-label="Settings"]';
const settingsButtons = `${settings} button`;
const sidebar = "aside.map-pane";
const queue = 'aside[aria-label="File open requests"] p[title]';
const heading = ".document-editor h1";
const selectedTab = 'button[role="tab"][aria-selected="true"]';
const goldField = 'table input[aria-label="Gold value"]';
const manualRequired =
  "Automatic bank discovery is unavailable. Select a folder.";
const readyStatus = "Bank inventory ready.";
const openingValue = "001";
const editedValue = "1,234";
const dirtyStatus = "Unsaved changes in 1 section";

function stage(message) {
  console.log(`Linux native manual root: ${message}`);
}

function inventoryReaders(evaluate) {
  const discovery = () =>
    evaluate(
      `const panels = document.querySelectorAll(arguments[0]);
      if (panels.length !== 1) return null;
      const groups = Array.from(panels[0].querySelectorAll("fieldset"))
        .filter((group) => group.querySelector(":scope > legend")?.textContent.trim() === "Bank discovery");
      if (groups.length !== 1) return null;
      const group = groups[0];
      const roots = group.querySelectorAll("p[title]");
      if (roots.length > 1) return null;
      return {
        root: roots.length === 1 ? {
          title: roots[0].getAttribute("title"), text: roots[0].textContent.trim(),
        } : null,
        statuses: Array.from(group.querySelectorAll('[role="status"]'), (item) => item.textContent.trim()),
        buttons: Array.from(group.querySelectorAll("button"), (button) => ({
          name: button.textContent.trim(), disabled: button.matches(":disabled"),
        })),
        accountSelectors: group.querySelectorAll("select").length,
        scanning: group.querySelector('progress[aria-label="Bank discovery progress"]') !== null,
      };`,
      settings,
    );
  const inventory = () =>
    evaluate(
      `const panes = document.querySelectorAll(arguments[0]);
      if (panes.length !== 1) return null;
      const pane = panes[0];
      return {
        roots: Array.from(pane.querySelectorAll("header p[title]"), (item) => item.getAttribute("title")),
        buttons: Array.from(pane.querySelectorAll("header button"), (button) => ({
          name: button.textContent.trim(), disabled: button.matches(":disabled"),
        })),
        statuses: Array.from(pane.querySelectorAll('[role="status"]'), (item) => item.textContent.trim()),
        maps: pane.querySelectorAll('ul[aria-label="Maps"]').length,
        bankLists: pane.querySelectorAll('ul[aria-label="Bank files"]').length,
        banks: Array.from(pane.querySelectorAll('ul[aria-label="Bank files"] li > button'), (button) => ({
          name: button.firstElementChild?.tagName === "SPAN" ? button.firstElementChild.textContent.trim() : null,
          unmatched: Array.from(button.children).filter((child) => child.textContent.trim() === "Unmatched map").length,
          scopes: Array.from(button.querySelectorAll("span[title]"), (item) => item.getAttribute("title")),
          current: button.getAttribute("aria-current"), disabled: button.matches(":disabled"),
        })),
        accountSelectors: pane.querySelectorAll("select").length,
        queued: document.querySelectorAll(arguments[1]).length,
      };`,
      sidebar,
      queue,
    );
  return { discovery, inventory };
}

function inventoryMatches(listed, collectionName, names, current = null) {
  return (
    listed?.roots.length === 1 &&
    listed.roots[0] === collectionName &&
    listed.maps === 0 &&
    listed.bankLists === 1 &&
    listed.accountSelectors === 0 &&
    listed.queued === 0 &&
    listed.banks.length === names.length &&
    listed.banks
      .map((bank) => bank.name)
      .sort()
      .every((name, index) => name === names[index]) &&
    listed.banks.every(
      (bank) =>
        bank.unmatched === 1 &&
        bank.scopes.length === 0 &&
        bank.current === (bank.name === current ? "true" : null) &&
        !bank.disabled,
    )
  );
}

export async function verifyManualRootSelection({
  command,
  waitFor,
  controls,
  currentName,
  collectionName,
  collectionPath,
  bankNames,
  refreshBankName,
  addRefreshBank,
  cancelChooser,
  chooseCollection,
  waitForChooserReleased,
  assertFilesUnchanged,
}) {
  assert.equal(typeof currentName, "string");
  assert.equal(typeof collectionName, "string");
  assert(currentName.length > 0 && collectionName.length > 0);
  assert.equal(typeof collectionPath, "string");
  assert(isAbsolute(collectionPath));
  // The native ready projection exposes file_name(), while the chooser uses the full path.
  assert.equal(basename(collectionPath), collectionName);
  assert(Array.isArray(bankNames) && bankNames.length === 2);
  assert(
    bankNames.every((name) => typeof name === "string" && name.length > 0),
  );
  assert.equal(new Set(bankNames).size, bankNames.length);
  assert(!bankNames.includes(currentName));
  assert.equal(typeof refreshBankName, "string");
  assert(refreshBankName.length > 0 && refreshBankName !== currentName);
  assert(!bankNames.includes(refreshBankName));
  assert.equal(typeof addRefreshBank, "function");
  const expectedNames = [...bankNames].sort();
  const refreshedNames = [...expectedNames, refreshBankName].sort();
  const {
    applyGold,
    click,
    clickButton,
    content,
    findControl,
    openInitialBank,
    stageGold,
    tableValue,
  } = controls;
  const evaluate = (script, ...args) =>
    command("/execute/sync", "POST", { script, args });
  const noQueuedBank = () =>
    evaluate(
      "return document.querySelectorAll(arguments[0]).length === 0;",
      queue,
    );
  const editorRetained = async () =>
    (await content(heading)) === currentName &&
    (await content(selectedTab)) === "Table View" &&
    (await tableValue()) === editedValue &&
    (await content(".editor-dirty-status")) === dirtyStatus &&
    (await content(".editor-draft-actions")) === null &&
    (await noQueuedBank()) &&
    (await findControl(".document-editor button", "Undo"));

  const { discovery, inventory } = inventoryReaders(evaluate);
  const ready = async (names = expectedNames) => {
    const state = await discovery();
    const listed = await inventory();
    return (
      state?.root?.title === collectionName &&
      state.root.text === `Bank folder ${collectionName}` &&
      state.statuses.length === 1 &&
      state.statuses[0] === readyStatus &&
      !state.scanning &&
      state.accountSelectors === 0 &&
      ["Select Folder…", "Refresh", "Use auto-detect"].every((name) =>
        state.buttons.some(
          (button) => button.name === name && !button.disabled,
        ),
      ) &&
      inventoryMatches(listed, collectionName, names)
    );
  };
  const returnToRetainedEditor = async (label) => {
    await clickButton("Back to editor");
    await waitFor(label, editorRetained);
    await assertFilesUnchanged();
  };
  const openSettings = () => click('button[aria-label="Settings"]', null);

  stage("Opening the current bank and applying its dirty edit.");
  await openInitialBank(currentName, openingValue);
  await stageGold(editedValue, { hasQueuedOpen: false });
  await applyGold();
  await waitFor("the applied current edit with enabled Undo", editorRetained);
  await assertFilesUnchanged();
  await openSettings();
  await waitFor("fresh Linux manual-only discovery", async () => {
    const state = await discovery();
    const listed = await inventory();
    return (
      state?.root === null &&
      state.statuses.length === 1 &&
      state.statuses[0] === manualRequired &&
      !state.scanning &&
      state.accountSelectors === 0 &&
      state.buttons.some(
        (button) => button.name === "Select Folder…" && !button.disabled,
      ) &&
      listed?.roots.length === 0 &&
      listed.bankLists === 0 &&
      listed.maps === 0 &&
      listed.accountSelectors === 0 &&
      listed.queued === 0
    );
  });
  const before = { discovery: await discovery(), inventory: await inventory() };

  stage("Opening the folder chooser and cancelling native selection.");
  await click(settingsButtons, "Select Folder…");
  await cancelChooser();
  await waitForChooserReleased();
  await waitFor("unchanged discovery after native folder Cancel", async () => {
    const after = {
      discovery: await discovery(),
      inventory: await inventory(),
    };
    return JSON.stringify(after) === JSON.stringify(before);
  });
  await assertFilesUnchanged();
  await returnToRetainedEditor(
    "the editable dirty bank and Undo after folder Cancel",
  );

  stage("Selecting the synthetic collection without opening an imported bank.");
  await openSettings();
  await click(settingsButtons, "Select Folder…");
  await chooseCollection();
  await waitForChooserReleased();
  await waitFor("the ready manual root and exact unmatched inventory", ready);
  await assertFilesUnchanged();
  await returnToRetainedEditor(
    "the original editable dirty bank after collection import",
  );

  stage("Adding a new synthetic bank before the deliberate Refresh.");
  await openSettings();
  await waitFor(
    "the original two imported banks before adding the third",
    ready,
  );
  await addRefreshBank();
  await assertFilesUnchanged();
  await waitFor("the original inventory before the explicit Refresh", ready);
  await click(settingsButtons, "Refresh");
  await waitFor("the refreshed manual root with exactly three banks", () =>
    ready(refreshedNames),
  );
  await assertFilesUnchanged();
  await returnToRetainedEditor(
    "the original editable dirty bank and Undo after Refresh",
  );
  stage("Native folder Cancel, manual collection import and Refresh passed.");
}

export async function verifyRestoredManualCollection({
  command,
  waitFor,
  controls,
  collectionName,
  collectionPath,
  bankNames,
  openBankName,
  openingValue,
  assertFilesUnchanged,
}) {
  assert.equal(typeof collectionName, "string");
  assert(collectionName.length > 0);
  assert.equal(typeof collectionPath, "string");
  assert(isAbsolute(collectionPath));
  assert.equal(basename(collectionPath), collectionName);
  assert(Array.isArray(bankNames) && bankNames.length === 3);
  assert(
    bankNames.every((name) => typeof name === "string" && name.length > 0),
  );
  assert.equal(new Set(bankNames).size, bankNames.length);
  assert(bankNames.includes(openBankName));
  assert.equal(typeof openingValue, "string");
  assert(openingValue.length > 0);
  const names = [...bankNames].sort();
  const { content, findControl, hasHistory, tableValue } = controls;
  const evaluate = (script, ...args) =>
    command("/execute/sync", "POST", { script, args });
  const { inventory } = inventoryReaders(evaluate);
  const ready = async (current = null) => {
    const listed = await inventory();
    return (
      inventoryMatches(listed, collectionName, names, current) &&
      listed.statuses.length === 1 &&
      listed.statuses[0] === "No cached maps found." &&
      ["Select Folder…", "Refresh", "Use auto-detect"].every((name) =>
        listed.buttons.some(
          (button) => button.name === name && !button.disabled,
        ),
      )
    );
  };

  stage("Waiting for the restored collection without a startup bank.");
  await waitFor(
    "the restored three-bank inventory and unopened welcome view",
    async () =>
      (await content(".editor-welcome h1")) === "StarBank Reloaded" &&
      (await content('.editor-welcome p[role="status"]')) === "No bank open." &&
      (await content(".document-editor")) === null &&
      (await ready()),
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await assertFilesUnchanged();

  stage("Opening the known bank from the restored inventory.");
  const row = await evaluate(
    `const rows = Array.from(document.querySelectorAll(arguments[0]))
      .filter((button) => button.firstElementChild?.tagName === "SPAN" &&
        button.firstElementChild.textContent.trim() === arguments[1]);
    if (rows.length !== 1 || rows[0].getClientRects().length === 0 ||
      rows[0].matches(":disabled") || rows[0].getAttribute("aria-disabled") === "true") return null;
    return rows[0];`,
    `${sidebar} ul[aria-label="Bank files"] li > button`,
    openBankName,
  );
  const elementKey = "element-6066-11e4-a52e-4f735466cecf";
  assert.equal(
    typeof row?.[elementKey],
    "string",
    "Missing unique restored bank row.",
  );
  await command(`/element/${row[elementKey]}/click`, "POST", {});
  await waitFor(
    "the clean editable bank opened from the restored collection",
    async () =>
      (await content(heading)) === openBankName &&
      (await content(selectedTab)) === "Table View" &&
      (await tableValue()) === openingValue &&
      (await findControl(goldField, null)) &&
      (await evaluate(
        "return document.querySelector(arguments[0])?.readOnly === false;",
        goldField,
      )) &&
      (await content(".editor-dirty-status")) === null &&
      (await content(".editor-draft-actions")) === null &&
      !(await hasHistory()) &&
      (await ready(openBankName)),
  );
  await assertFilesUnchanged();
  stage("Restart restored the manual root and opened the known bank cleanly.");
}
