import assert from "node:assert/strict";

async function openStructure(page) {
  await page
    .getByRole("button", { name: "Edit structure…", exact: true })
    .click();
  return page.getByRole("dialog", { name: "Edit bank structure", exact: true });
}

function checkUnloadGuard(page, guarded) {
  return page.waitForFunction(
    (expected) =>
      !window.dispatchEvent(new Event("beforeunload", { cancelable: true })) ===
      expected,
    guarded,
  );
}

async function checkFrozen(form, labels) {
  await form
    .getByLabel("Action", { exact: true })
    .and(form.locator(":disabled"))
    .waitFor();
  for (const label of labels)
    assert(await form.getByLabel(label, { exact: true }).isDisabled(), label);
}

export async function verifyStructureDraftClose(page, tableValue) {
  const openingValue = await tableValue.inputValue();
  const form = await openStructure(page);
  const name = form.getByRole("textbox", { name: "Name", exact: true });
  const close = form.getByRole("button", { name: "Close", exact: true });
  await name.waitFor();
  await checkUnloadGuard(page, false);

  await name.fill("Pending section");
  await checkUnloadGuard(page, true);
  await name.fill("");
  await checkUnloadGuard(page, false);

  await name.fill("Another unsent section");
  await checkUnloadGuard(page, true);
  await form.getByLabel("Action", { exact: true }).selectOption("add_value");
  await checkUnloadGuard(page, false);
  await form.getByLabel("Value type", { exact: true }).selectOption("int");
  await checkUnloadGuard(page, true);
  const value = form.getByRole("textbox", { name: "Value", exact: true });
  await value.fill("1,,2");
  await form.getByRole("button", { name: "Add value", exact: true }).click();
  await form
    .getByRole("status")
    .filter({ hasText: /integer/u })
    .waitFor();
  assert.equal(await value.inputValue(), "1,,2");
  await checkUnloadGuard(page, true);

  await close.click();
  await form.waitFor({ state: "hidden" });
  await checkUnloadGuard(page, false);
  assert.equal(await tableValue.inputValue(), openingValue);
}

export async function verifyNativeStructureDraftClose(page, bridge) {
  await page.waitForFunction(
    () => typeof window.fixtureCloseProbe === "function",
  );
  const form = await openStructure(page);
  const name = form.getByRole("textbox", { name: "Name", exact: true });

  async function probe(ticket, choice, hasDraft, whilePending) {
    bridge.chooseClose(choice);
    const release = whilePending && bridge.holdNextCloseConfirmation();
    await page.evaluate(
      (ticket) => window.fixtureCloseProbe({ ticket }),
      ticket,
    );
    if (release) {
      try {
        await page.waitForFunction(
          (ticket) => window.fixtureRequestedCloseTicket === ticket,
          ticket,
        );
        await whilePending();
      } finally {
        release();
      }
    }
    await page.waitForFunction(
      (ticket) => window.fixtureCompletedCloseTicket === ticket,
      ticket,
    );
    const { report } = bridge.callsFor("confirm_window_close").at(-1);
    assert.equal(report.ticket, ticket);
    assert.equal(report.hasDraft, hasDraft);
    assert.equal(report.canSave, !hasDraft);
    assert.deepEqual(bridge.callsFor("complete_window_close").at(-1), {
      ticket,
      draftGeneration: report.draftGeneration,
      proceed: choice === "discard",
    });
    return report.draftGeneration;
  }

  bridge.choose("save", "success");
  await name.fill("Pending section");
  // An unexpected Save reply cannot commit while a draft is present.
  const first = await probe(1, "save", true, async () => {
    await checkFrozen(form, ["Name"]);
    assert(
      await form.getByRole("button", { name: "Add section" }).isDisabled(),
    );
  });
  assert.equal(await name.inputValue(), "Pending section");
  await name.and(form.locator(":enabled")).waitFor();
  await name.fill("Revised pending section");
  const second = await probe(2, "save", true);
  assert(second > first);
  assert.equal(await name.inputValue(), "Revised pending section");

  await form.getByRole("button", { name: "Close", exact: true }).click();
  // The dialog hides before its close event clears the draft in React.
  await page
    .locator("dialog")
    .filter({ hasText: "Edit bank structure" })
    .waitFor({ state: "detached" });
  assert((await probe(3, "discard", false)) > second);
  assert.deepEqual(bridge.callsFor("save_active_bank"), []);
  assert.deepEqual(bridge.callsFor("save_bank_as"), []);
  assert.equal(
    await page.getByRole("table").getByRole("textbox").inputValue(),
    "001",
  );

  const editing = await openStructure(page);
  await editing.getByLabel("Action", { exact: true }).selectOption("add_value");
  await editing.getByLabel("Value type", { exact: true }).selectOption("int");
  const value = editing.getByLabel("Value", { exact: true });
  const apply = editing.getByRole("button", { name: "Add value", exact: true });
  await value.fill("1,,2");
  const { started, release } = bridge.holdNextStructureReply();
  try {
    await apply.click();
    await started;
    await checkFrozen(editing, [
      "Section",
      "Key",
      "Element name",
      "Value type",
      "Value",
    ]);
  } finally {
    release();
  }
  await editing
    .getByRole("status")
    .filter({ hasText: /integer/u })
    .waitFor();
  assert.equal(await value.inputValue(), "1,,2");
  await value.fill("007");
  await apply.click();
  await editing.waitFor({ state: "hidden" });
  await page.getByRole("tab", { name: "Raw View", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Bank XML", exact: true })
    .filter({ hasText: 'int="007"' })
    .waitFor();
}
