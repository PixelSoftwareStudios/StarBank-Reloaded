import assert from "node:assert/strict";

const originalName = "Synthetic.SC2Bank";
const destination = {
  display: "/synthetic/recorded-only/Verified Save.SC2Bank",
  truncated: false,
  lossy: false,
};
const verifiedReceipt = {
  operation: "save_overwrite",
  outcome: "verified_needs_reopen",
  affectsActive: true,
  destination,
  recovery: { kind: "unknown" },
  backup: { kind: "not_created" },
  reason: "Synthetic editor update failure after a verified commit.",
  reasonTruncated: false,
};

function button(page, name) {
  return page.getByRole("button", { name, exact: true });
}

function gold(page) {
  return page.getByRole("table").getByRole("textbox");
}

function heading(page, name) {
  return page.getByRole("heading", { name, exact: true });
}

function recoveryDetails(page) {
  return page.getByRole("dialog", {
    name: "Write recovery details",
    exact: true,
  });
}

function currentScope(bridge) {
  const { document_id, expected_revision } = bridge.wireRequests.findLast(
    ({ command }) => command.kind === "raw_text",
  );
  return { document_id, expected_revision };
}

function fixtureCommand(bridge, target, command) {
  return bridge.dispatchFixture({
    schema_version: 1,
    request_id: "verified-reopen-fixture",
    ...target,
    command,
  });
}

async function applyValue(page, value) {
  await gold(page).fill(value);
  await button(page, "Apply edits").click();
  await button(page, "Undo").click({ trial: true });
}

async function inspectDetails(page) {
  const dialog = recoveryDetails(page);
  await dialog
    .getByText(/verified the write on disk.*reopen the destination/i)
    .waitFor();
  assert.equal(
    await dialog
      .getByRole("textbox", { name: "Destination path" })
      .inputValue(),
    destination.display,
  );
  await button(dialog, "Close").click();
  await dialog.waitFor({ state: "hidden" });
}

async function checkFrozenEditor(page, value, primaryOutput = "Save As…") {
  await page.getByRole("cell", { name: value, exact: true }).waitFor();
  assert.equal(await gold(page).count(), 0);
  assert(await button(page, "Open Bank…").isDisabled());
  assert(await button(page, primaryOutput).isDisabled());
  assert(await button(page, "Undo").isDisabled());

  const tools = button(page, "Tools");
  await tools.click();
  const menu = page.getByRole("menu");
  for (const name of ["Re-sign external bank…", "Back up all bank files"]) {
    assert.equal(
      await menu.getByRole("menuitem", { name }).getAttribute("aria-disabled"),
      "true",
    );
  }
  await menu.getByRole("menuitem", { name: "Review write recovery…" }).click();
  await inspectDetails(page);
}

function checkPickerScope(bridge, target) {
  const [args] = bridge.callsFor("open_bank").slice(-1);
  assert.deepEqual(Object.keys(args), ["control"]);
  const control = JSON.parse(args.control);
  assert.equal(control.document_id, undefined);
  assert.equal(control.expected_revision, undefined);
  if (target) {
    const confirmation = bridge.callsFor("confirm_bank_navigation").at(-1);
    assert.deepEqual(confirmation, {
      target: {
        documentId: target.document_id,
        revision: target.expected_revision,
        requestId: control.request_id,
      },
      canSave: false,
    });
    assert.deepEqual(control.command, {
      kind: "open",
      payload: { discard_current: true },
    });
  } else {
    assert.deepEqual(control.command, { kind: "open" });
  }
}

export async function verifyVerifiedNeedsReopen(page, bridge) {
  await button(page, "Open Bank…").click();
  await heading(page, originalName).waitFor();
  await applyValue(page, "002");
  const displayed = currentScope(bridge);
  const readsBeforeWrite = bridge.wireRequests.length;
  let actual;
  bridge.failNextWrite("save_active_bank", () => {
    const snapshot = fixtureCommand(bridge, displayed, { kind: "snapshot" });
    assert.equal(snapshot.result.kind, "snapshot");
    const id = snapshot.result.payload.sections[0].keys[0].values[0].id;
    const reply = fixtureCommand(bridge, displayed, {
      kind: "set_values",
      payload: { edits: [{ id, value: "003" }] },
    });
    assert.equal(reply.result.kind, "snapshot");
    assert.equal(reply.result.payload.dirty, true);
    actual = {
      document_id: reply.document_id,
      expected_revision: reply.revision,
    };
    assert.notEqual(actual.expected_revision, displayed.expected_revision);
    return { ...verifiedReceipt, editorTarget: actual };
  });

  await button(page, "Save").click();
  await inspectDetails(page);
  const reopen = button(page, "Reopen bank…");
  await reopen.waitFor();
  await checkFrozenEditor(page, "002");
  assert.equal(bridge.callsFor("open_bank").length, 1);
  assert.equal(bridge.wireRequests.length, readsBeforeWrite);

  bridge.choose("cancel");
  await reopen.click();
  await reopen.click({ trial: true });
  assert.deepEqual(
    bridge.callsFor("confirm_bank_navigation").map(({ target, canSave }) => ({
      documentId: target.documentId,
      revision: target.revision,
      canSave,
    })),
    [
      {
        documentId: actual.document_id,
        revision: actual.expected_revision,
        canSave: false,
      },
    ],
  );
  assert.equal(bridge.callsFor("open_bank").length, 1);

  bridge.choose("discard");
  bridge.cancelNextPicker();
  await reopen.click();
  await reopen.click({ trial: true });
  assert.equal(bridge.callsFor("open_bank").length, 2);
  checkPickerScope(bridge, actual);
  await checkFrozenEditor(page, "002");

  bridge.failNextOpenProjection();
  await reopen.click();
  const finish = button(page, "Finish opening bank");
  await finish.waitFor();
  assert.equal(bridge.callsFor("open_bank").length, 3);
  checkPickerScope(bridge, actual);
  assert.equal(await reopen.count(), 0);
  await checkFrozenEditor(page, "002");
  const accepted = currentScope(bridge);
  assert.notEqual(accepted.document_id, actual.document_id);
  const beforeFinish = bridge.wireRequests.length;
  await finish.click();
  await heading(page, "Replacement-2.SC2Bank").waitFor();
  assert.deepEqual(
    bridge.wireRequests
      .slice(beforeFinish)
      .map(({ command, document_id, expected_revision }) => ({
        kind: command.kind,
        document_id,
        expected_revision,
      })),
    [
      { kind: "snapshot", ...accepted },
      { kind: "raw_text", ...accepted },
    ],
  );
  assert.equal(bridge.callsFor("open_bank").length, 3);
  assert.equal(bridge.callsFor("save_active_bank").length, 1);
  assert.equal(await gold(page).inputValue(), "001");

  // Trigger the receipt inside the conflict dialog, where the outer Save is still pending.
  await applyValue(page, "009");
  const oldEditor = currentScope(bridge);
  bridge.choose(undefined, "external");
  await button(page, "Save").click();
  const conflict = page.getByRole("dialog", { name: "Bank changed on disk" });
  await conflict.waitFor();
  bridge.failNextWrite("save_bank_as", () => {
    const closed = fixtureCommand(bridge, oldEditor, {
      kind: "close",
      payload: { discard: true },
    });
    assert.equal(closed.result.kind, "closed");
    return { ...verifiedReceipt, operation: "save_as", affectsActive: false };
  });
  await button(conflict, "Save As…").click();
  await inspectDetails(page);
  await button(conflict, "Keep Editing").click();
  await conflict.waitFor({ state: "hidden" });
  await checkFrozenEditor(page, "009", "Save");

  const confirms = bridge.callsFor("confirm_bank_navigation").length;
  await reopen.click();
  await heading(page, "Replacement-3.SC2Bank").waitFor();
  assert.equal(bridge.callsFor("confirm_bank_navigation").length, confirms);
  checkPickerScope(bridge);
  assert.equal(await gold(page).inputValue(), "001");
  assert(await button(page, "Save").isEnabled());
  assert.equal(await reopen.count(), 0);
  await button(page, "Tools").click();
  await page.getByRole("menu").waitFor();
  assert.equal(
    await page
      .getByRole("menuitem", { name: "Review write recovery…" })
      .count(),
    0,
  );
  await page.keyboard.press("Escape");
}
