import assert from "node:assert/strict";

const openingGold = "001";
const editedGold = "1,234";
const dirtyStatus = "Unsaved changes in 1 section";
const copiedStatus = "Copied bank XML.";

export async function verifyClipboardXml({
  command,
  waitFor,
  controls,
  assertInputsUnchanged,
  assertClipboard,
  name,
  expectedXml,
}) {
  const {
    openInitialBank,
    stageGold,
    applyGold,
    click,
    content,
    tableValue,
    findControl,
  } = controls;

  const observeCopy = () =>
    command("/execute/sync", "POST", {
      script: `
        const status = document.querySelector(".editor-status");
        const actionText = Array.from(status?.childNodes ?? [])
          .filter((node) => node.nodeType === Node.TEXT_NODE)
          .map((node) => node.textContent).join("").trim();
        return {
          copied: actionText === arguments[0],
          dialogs: Array.from(document.querySelectorAll("dialog[open]"), (dialog) =>
            document.getElementById(dialog.getAttribute("aria-labelledby"))?.textContent.trim() ?? "Unnamed dialog"),
        };`,
      args: [copiedStatus],
    });

  await openInitialBank(name, openingGold);
  await stageGold(editedGold, { hasQueuedOpen: false });
  await applyGold();
  assert.deepEqual(
    await observeCopy(),
    { copied: false, dialogs: [] },
    "Copy must begin without a previous success status or an open dialog.",
  );

  await click('button[aria-label="More save options"]', null);
  await click('[role="menuitem"]', "Copy bank XML to clipboard");
  const outcome = await waitFor("the native Copy XML outcome", async () => {
    const observed = await observeCopy();
    return observed.copied || observed.dialogs.length > 0 ? observed : null;
  });
  assert.deepEqual(
    outcome,
    { copied: true, dialogs: [] },
    "The native clipboard must report success without a fallback or other dialog.",
  );
  await waitFor(
    "the edited dirty bank with Undo retained after Copy",
    async () =>
      (await content(".document-editor h1")) === name &&
      (await tableValue()) === editedGold &&
      (await content(".editor-dirty-status")) === dirtyStatus &&
      (await findControl("button", "Undo")),
  );
  await assertInputsUnchanged();
  await assertClipboard(expectedXml);
}
