import assert from "node:assert/strict";

const dialog = "dialog[open]";
const tools = 'button[aria-label="Tools"]';
const toolsMenu = '[role="menu"][aria-label="Bank tools"]';
const queue = 'aside[aria-label="File open requests"]';
const heading = ".document-editor h1";
const selectedTab = 'button[role="tab"][aria-selected="true"]';
const editorStatus = ".editor-status";
const setting = 'section[aria-label="Settings"] input[type="checkbox"]';
const autoResign = "Auto re-sign banks on save";
const formFields = `${dialog} fieldset input`;
const elementKey = "element-6066-11e4-a52e-4f735466cecf";
const openingValue = "001";
const editedValue = "1,234";
const editedStatus = "Unsaved changes in 1 section";
const outputName = "Foreign signed.sc2bank";
const context = [
  ["Map author handle", "1-S2-1-9911"],
  ["Player handle", "1-S2-1-9922"],
  ["Bank name without extension", "DigestBank"],
];

export async function verifyExternalSigning({
  command,
  waitFor,
  controls,
  activeName,
  foreignName,
  chooseSource,
  waitForSourceChooserReleased,
  chooseDestination,
  waitForDestinationChooserReleased,
  assertFilesUnchanged,
  assertNoOutput,
  assertSignedOutput,
}) {
  const {
    applyGold,
    click,
    clickButton,
    content,
    disableAutoResign,
    evaluate,
    findControl,
    hasHistory,
    openInitialBank,
    stageGold,
    tableValue,
  } = controls;
  const readContext = () =>
    evaluate(
      `return Array.from(document.querySelectorAll(arguments[0]), (field) =>
        [field.labels?.[0]?.textContent.trim(), field.value]);`,
      formFields,
    );
  const editorRetained = async () =>
    (await content(heading)) === activeName &&
    (await content(selectedTab)) === "Table View" &&
    (await tableValue()) === editedValue &&
    (await content(".editor-dirty-status")) === editedStatus &&
    (await content(".editor-draft-actions")) === null &&
    !(await content(`${queue} p[title]`)) &&
    (await hasHistory());
  const undoRetained = async () =>
    (await editorRetained()) &&
    (await findControl(".document-editor button", "Undo"));

  await openInitialBank(activeName, openingValue);
  await disableAutoResign();
  await stageGold(editedValue, { hasQueuedOpen: false });
  await applyGold();
  await waitFor("the active dirty Gold bank and available Undo", undoRetained);
  await assertFilesUnchanged();
  await assertNoOutput();

  await click(tools, null);
  await click(`${toolsMenu} [role="menuitem"]`, "Re-sign external bank…");
  await chooseSource();
  await waitFor("the unsigned external bank in the signing form", () =>
    evaluate(
      `const [selector, name] = arguments;
      const dialog = document.querySelector(selector);
      const label = dialog?.getAttribute("aria-labelledby");
      const summary = dialog?.getAttribute("aria-describedby");
      return document.getElementById(label)?.textContent.trim() === "Re-sign external bank" &&
        document.getElementById(summary)?.firstElementChild?.textContent.trim() === name + " · Unsigned";`,
      dialog,
      foreignName,
    ),
  );
  await waitForSourceChooserReleased();
  assert.deepEqual(
    await readContext(),
    context.map(([label]) => [label, ""]),
  );
  for (const [label, value] of context) {
    const field = await waitFor(`the enabled ${label} field`, () =>
      findControl(formFields, label),
    );
    assert.equal(
      typeof field[elementKey],
      "string",
      "Missing WebDriver input ID.",
    );
    await click(formFields, label);
    await command(`/element/${field[elementKey]}/value`, "POST", {
      text: value,
    });
  }
  assert.deepEqual(await readContext(), context);

  await click(`${dialog} button`, "Check signature");
  await waitFor(
    "the external bank's unsigned check result",
    async () =>
      (await content(`${dialog} p[role="status"]`)) ===
        "This bank has no stored signature." &&
      (await findControl(`${dialog} button`, "Save signed copy…")),
  );
  assert(
    await editorRetained(),
    "Checking the external bank changed the active edit.",
  );
  await assertFilesUnchanged();
  await assertNoOutput();

  await click(`${dialog} button`, "Save signed copy…");
  await chooseDestination();
  await waitFor("the verified signed-copy notice and closed signing form", () =>
    evaluate(
      `const [dialog, status, message] = arguments;
      const footer = document.querySelector(status);
      return !document.querySelector(dialog) && Array.from(footer?.childNodes ?? [])
        .some((node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim() === message);`,
      dialog,
      editorStatus,
      `Saved verified signed copy ${outputName}.`,
    ),
  );
  await waitForDestinationChooserReleased();
  await assertSignedOutput();
  await assertFilesUnchanged();
  await waitFor(
    "the original dirty bank with Undo after signing the copy",
    undoRetained,
  );

  await click('button[aria-label="Settings"]', null);
  await waitFor("the persisted disabled auto re-sign setting", () =>
    findControl(setting, autoResign, false),
  );
  await clickButton("Back to editor");
  await waitFor(
    "the unchanged active edit after checking Settings",
    undoRetained,
  );
  await assertFilesUnchanged();
  console.log(
    "Linux native external signing: unsigned check, verified new copy and retained active edit passed.",
  );
}
