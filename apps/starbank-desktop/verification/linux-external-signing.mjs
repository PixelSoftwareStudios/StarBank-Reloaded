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

function stage(message) {
  console.log(`Linux native external signing: ${message}`);
}

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

  stage("Opening the queued active bank.");
  await openInitialBank(activeName, openingValue);
  stage("Active Table opened. Turning off auto re-sign.");
  await disableAutoResign();
  stage("Auto re-sign is off. Staging the active edit.");
  await stageGold(editedValue, { hasQueuedOpen: false });
  stage("Active draft staged. Applying it.");
  await applyGold();
  await waitFor("the active dirty Gold bank and available Undo", undoRetained);
  await assertFilesUnchanged();
  await assertNoOutput();

  stage("Active edit confirmed. Opening Tools.");
  await click(tools, null);
  stage("Tools trigger clicked. Selecting external re-sign.");
  await click(`${toolsMenu} [role="menuitem"]`, "Re-sign external bank…");
  stage("External re-sign selected. Submitting the GTK source path.");
  await chooseSource();
  stage("Source path submitted. Waiting for the signing form.");
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
  stage("Unsigned signing form visible. Checking source chooser release.");
  await waitForSourceChooserReleased();
  stage("Source chooser released. Entering signing context.");
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

  stage("Signing context confirmed. Checking the stored signature.");
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

  stage("Unsigned check confirmed. Opening the signed-copy chooser.");
  await click(`${dialog} button`, "Save signed copy…");
  stage("Save control clicked. Submitting the GTK destination path.");
  await chooseDestination();
  stage("Destination path submitted. Waiting for the signed-copy result.");
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
  stage("Verified-copy notice visible. Checking destination chooser release.");
  await waitForDestinationChooserReleased();
  await assertSignedOutput();
  await assertFilesUnchanged();
  await waitFor(
    "the original dirty bank with Undo after signing the copy",
    undoRetained,
  );

  stage("Output and active edit confirmed. Reopening Settings.");
  await click('button[aria-label="Settings"]', null);
  await waitFor("the persisted disabled auto re-sign setting", () =>
    findControl(setting, autoResign, false),
  );
  stage("Auto re-sign remains off. Returning to the editor.");
  await clickButton("Back to editor");
  await waitFor(
    "the unchanged active edit after checking Settings",
    undoRetained,
  );
  await assertFilesUnchanged();
  stage("Unsigned check, verified new copy and retained active edit passed.");
}
