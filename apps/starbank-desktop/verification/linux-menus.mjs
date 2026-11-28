import assert from "node:assert/strict";

const trigger = 'button[aria-label="More save options"]';
const saveMenu = '[role="menu"][aria-label="Save options"]';
const heading = ".document-editor h1";
const queue = 'aside[aria-label="File open requests"]';
const selectedTab = 'button[role="tab"][aria-selected="true"]';
const maxTabSteps = 24;

// WebDriver key values for the installed WebView.
const tab = "\uE004";
const escapeKey = "\uE00C";
const end = "\uE010";
const home = "\uE011";
const arrowUp = "\uE013";
const arrowDown = "\uE015";

const copy = "Copy bank XML to clipboard";
const backups = "Bank Backups…";
const exportCopy = "Export copy…";

export async function verifySaveMenu({
  command,
  waitFor,
  controls,
  assertInputsUnchanged,
}) {
  const {
    click,
    clickTab,
    content,
    evaluate,
    findControl,
    hasHistory,
    tableValue,
  } = controls;
  const press = (value) =>
    command("/actions", "POST", {
      actions: [
        {
          type: "key",
          id: "save-menu-keyboard",
          actions: [
            { type: "keyDown", value },
            { type: "keyUp", value },
          ],
        },
      ],
    });
  const focusedControl = () =>
    evaluate(
      `const active = document.activeElement;
      return {
        trigger: active?.matches(arguments[0]) ?? false,
        name: active?.getAttribute("aria-label") ||
          (active?.matches('button, [role="menuitem"]') ? active.textContent.trim() : active?.tagName),
      };`,
      trigger,
    );
  const menuClosed = () =>
    evaluate(
      `const [trigger, menu] = arguments;
      return document.querySelector(trigger)?.getAttribute("aria-expanded") === "false" &&
        !document.querySelector(menu);`,
      trigger,
      saveMenu,
    );
  const focusedItem = (name) =>
    waitFor(`${name} focused in the native Save menu`, () =>
      evaluate(
        `const [triggerSelector, menuSelector, name] = arguments;
        const trigger = document.querySelector(triggerSelector);
        const menu = document.getElementById(trigger?.getAttribute("aria-controls"));
        const active = document.activeElement;
        return trigger?.getAttribute("aria-expanded") === "true" && menu?.matches(menuSelector) &&
          menu.contains(active) && active.matches('[role="menuitem"]') &&
          active.textContent.trim() === name;`,
        trigger,
        saveMenu,
        name,
      ),
    );
  const editorState = async () => ({
    title: await content(heading),
    selected: await content(selectedTab),
    gold: await tableValue(),
    dirty: await content(".editor-dirty-status"),
    draft: await content(".editor-draft-actions"),
    history: await hasHistory(),
    queue: await evaluate(
      `return Array.from(document.querySelectorAll(arguments[0] + " p[title]"),
        (entry) => [entry.title, entry.textContent.trim()]);`,
      queue,
    ),
  });

  const before = await editorState();
  assert(
    before.title && before.queue.length > 0,
    "Expected a loaded bank and a queued bank.",
  );
  assert.equal(before.selected, "Table View");
  assert.equal(before.gold, "001");
  assert.equal(before.dirty, null);
  assert.equal(before.draft, null);
  assert.equal(before.history, false);
  await waitFor("the enabled Linux Save As primary action", () =>
    findControl(".editor-save-split button", "Save As…"),
  );
  assert(await menuClosed());

  await clickTab("Table View");
  const tabStops = [];
  let focus = await focusedControl();
  for (let index = 0; index < maxTabSteps && !focus.trigger; index++) {
    tabStops.push(focus.name);
    await press(tab);
    focus = await focusedControl();
  }
  assert(
    focus.trigger,
    `Tab did not reach the native Save menu trigger. Focus path: ${JSON.stringify([...tabStops, focus.name])}`,
  );

  await press(arrowDown);
  await focusedItem(copy);
  const rows = await evaluate(
    `return Array.from(document.querySelector(arguments[0]).querySelectorAll('[role="menuitem"], [role="separator"]'), (row) => {
      const bounds = row.getBoundingClientRect();
      const visible = bounds.width > 0 && bounds.height > 0 && getComputedStyle(row).visibility === "visible";
      if (!visible) return null;
      if (row.getAttribute("role") === "separator") return { separator: true };
      return {
        label: row.textContent.trim(),
        enabled: !row.matches(":disabled") && row.getAttribute("aria-disabled") !== "true" &&
          !row.hasAttribute("data-disabled"),
      };
    });`,
    saveMenu,
  );
  assert.deepEqual(rows, [
    { label: copy, enabled: true },
    { separator: true },
    { label: backups, enabled: true },
    { label: "Re-sign Bank", enabled: true },
    { label: exportCopy, enabled: true },
  ]);

  for (const [key, item] of [
    [arrowDown, backups],
    [arrowUp, copy],
    [end, exportCopy],
    [home, copy],
  ]) {
    await press(key);
    await focusedItem(item);
  }
  await press(escapeKey);
  await waitFor(
    "the closed Save menu and restored trigger focus",
    async () => (await menuClosed()) && (await focusedControl()).trigger,
  );

  await press(arrowDown);
  await focusedItem(copy);
  await click(heading, null);
  await waitFor(
    "outside pointer dismissal of the native Save menu",
    menuClosed,
  );
  assert.deepEqual(await editorState(), before);
  await assertInputsUnchanged();
  console.log(
    "Linux native Save menu: order, keyboard focus, Escape and outside dismissal passed.",
  );
}
