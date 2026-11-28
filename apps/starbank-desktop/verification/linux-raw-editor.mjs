import assert from "node:assert/strict";

const rawXml = '.cm-content[aria-label="Bank XML"]';
const editableRaw = `${rawXml}[contenteditable="true"]`;
const queue = 'aside[aria-label="File open requests"]';
const selectedTab = 'button[role="tab"][aria-selected="true"]';
const draftActions = ".editor-draft-actions";
const dirtyStatus = ".editor-dirty-status";
const syntaxHint = "Possible XML syntax issue";

// WebDriver's special key values. Printable characters use their literal text.
const backspace = "\uE003";
const shift = "\uE008";
const control = "\uE009";
const end = "\uE010";
const left = "\uE012";
const down = (value) => ({ type: "keyDown", value });
const up = (value) => ({ type: "keyUp", value });
const press = (value) => [down(value), up(value)];
const documentEnd = [down(control), ...press(end), up(control)];

export async function verifyRawEditing({
  command,
  waitFor,
  controls,
  openingXml,
  editedXml,
  assertInputsUnchanged,
  expectedQueue,
}) {
  const {
    click,
    clickButton,
    clickTab,
    content,
    evaluate,
    findControl,
    hasHistory,
    queuedName,
    tableValue,
  } = controls;
  const openingValue = "001";
  const editedValue = "4321";
  const attributePrefix = 'int="';
  const attribute = openingXml.indexOf(`${attributePrefix}${openingValue}"`);
  assert.notEqual(attribute, -1, "Opening Raw fixture must contain Gold 001");
  const valueStart = attribute + attributePrefix.length;
  const trailingXml = openingXml.slice(valueStart + openingValue.length);
  assert.equal(
    editedXml,
    openingXml.slice(0, valueStart) + editedValue + trailingXml,
  );
  assert(editedXml.endsWith("</Bank>"));
  const brokenXml = editedXml.slice(0, -1);

  const typeKeys = (actions) =>
    command("/actions", "POST", {
      actions: [{ type: "key", id: "raw-keyboard", actions }],
    });
  const text = () =>
    evaluate(
      "return document.querySelector(arguments[0])?.textContent ?? null;",
      rawXml,
    );
  const hint = () =>
    evaluate(
      'return document.querySelector(arguments[0])?.getAttribute("aria-description") ?? null;',
      rawXml,
    );
  const rawFocused = () =>
    evaluate(
      'return !document.querySelector("dialog[open]") && document.activeElement?.matches(arguments[0]);',
      editableRaw,
    );
  const queueRetained = async () =>
    (await queuedName(expectedQueue.name)) === expectedQueue.label;

  assert.equal(await tableValue(), openingValue);
  await clickTab("Raw View");
  await waitFor(
    "the opening XML in the editable native Raw view",
    async () =>
      (await findControl(editableRaw, null)) && (await text()) === openingXml,
  );
  await click(editableRaw, null);
  await waitFor("keyboard focus in the editable Raw view", rawFocused);
  // Edit the existing attribute by keyboard so CodeMirror's tag completion does
  // not transform a newly typed copy of the XML document.
  const moveToValue = Array.from({ length: trailingXml.length }, () =>
    press(left),
  ).flat();
  const selectValue = Array.from({ length: openingValue.length }, () =>
    press(left),
  ).flat();
  await typeKeys([
    ...documentEnd,
    ...moveToValue,
    down(shift),
    ...selectValue,
    up(shift),
    ...Array.from(editedValue).flatMap(press),
  ]);
  await waitFor(
    "the exact XML value typed by keyboard",
    async () => (await text()) === editedXml,
  );
  await typeKeys([...documentEnd, ...press(backspace)]);
  await waitFor(
    "the malformed Raw draft and syntax hint",
    async () =>
      (await text()) === brokenXml &&
      (await hint()) === syntaxHint &&
      (await content(`${draftActions} [role="status"]`)) ===
        "Unapplied Raw View changes",
  );

  await clickTab("Table View");
  await waitFor("the native engine rejection of the malformed Raw draft", () =>
    evaluate(`
      const dialog = Array.from(document.querySelectorAll("dialog[open]"))
        .find((dialog) => dialog.querySelector("h2")?.textContent === "Raw draft could not be applied");
      return dialog?.querySelector("p")?.textContent === "The bank XML could not be opened safely.";
    `),
  );
  assert.equal(await text(), brokenXml);
  assert.equal(await content(selectedTab), "Raw View");
  assert.equal(await content(dirtyStatus), null);
  assert.equal(await hasHistory(), false);
  assert(await queueRetained());
  assert.equal(
    await evaluate(
      `return Array.from(document.querySelectorAll(arguments[0] + " button"))
        .find((button) => button.textContent.trim() === "Open")?.disabled;`,
      queue,
    ),
    true,
  );
  await assertInputsUnchanged();

  await click("dialog[open] button", "Repair XML");
  await waitFor(
    "keyboard focus returned to the retained Raw draft",
    rawFocused,
  );
  assert.equal(await text(), brokenXml);
  await typeKeys([...documentEnd, ...press(">")]);
  await waitFor(
    "the exact repaired XML with its syntax hint cleared",
    async () => (await text()) === editedXml && (await hint()) === null,
  );
  await clickButton("Apply XML");
  // Raw replacement gives the changed section a new ID. The engine counts the
  // prior occurrence as removed and the new occurrence as added.
  await waitFor(
    "the accepted Raw revision",
    async () =>
      (await content(selectedTab)) === "Raw View" &&
      (await text()) === editedXml &&
      (await content(draftActions)) === null &&
      (await content(dirtyStatus)) === "Unsaved changes in 2 sections",
  );
  await clickTab("Table View");
  await waitFor(
    "the Table projection of the accepted Raw revision",
    async () =>
      (await content(selectedTab)) === "Table View" &&
      (await tableValue()) === "4,321",
  );
  assert(await queueRetained());
  await assertInputsUnchanged();

  await clickButton("Undo");
  await waitFor(
    "the opening Table value and clean state after one Undo",
    async () =>
      (await tableValue()) === openingValue &&
      (await content(dirtyStatus)) === null &&
      (await content(draftActions)) === null &&
      !(await findControl("button", "Undo")) &&
      (await findControl("button", "Redo")),
  );
  assert(await queueRetained());
  await waitFor("the unchanged queued Open after Undo", () =>
    findControl(`${queue} button`, "Open"),
  );
  await assertInputsUnchanged();
  console.log(
    "Linux native Raw XML rejection, keyboard repair, Apply and Undo passed.",
  );
}
