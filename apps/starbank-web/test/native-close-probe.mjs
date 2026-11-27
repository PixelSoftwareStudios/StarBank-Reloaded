import assert from "node:assert/strict";

function openButton(page) {
  return page.getByRole("button", { name: "Open Bank…", exact: true });
}

function closeProbe(page, ticket) {
  return page.evaluate(
    (ticket) => window.fixtureCloseProbe({ ticket }),
    ticket,
  );
}

export async function verifyEmptyEngineCloseProbe(page, bridge) {
  const open = openButton(page);
  const busyNotice = page.getByRole("status").filter({
    hasText: "Wait for the current bank action to finish before closing.",
  });

  await page.waitForFunction(
    () => typeof window.fixtureCloseProbe === "function",
  );
  await open.click();
  await page.getByRole("heading", { name: "Synthetic.SC2Bank" }).waitFor();
  const { document_id, expected_revision } = bridge.wireRequests.findLast(
    ({ command }) => command.kind === "raw_text",
  );
  const closed = bridge.dispatchFixture({
    schema_version: 1,
    request_id: "close-probe-fixture",
    document_id,
    expected_revision,
    command: { kind: "close" },
  });
  assert.equal(closed.result.kind, "closed");

  const releasePicker = bridge.holdNextPicker();
  await open.click();
  await page.getByRole("status").filter({ hasText: "Working…" }).waitFor();
  await closeProbe(page, 1);
  releasePicker();
  await busyNotice.waitFor();
  assert.deepEqual(bridge.callsFor("confirm_window_close"), []);
  assert.deepEqual(bridge.callsFor("complete_window_close"), []);

  bridge.chooseClose("discard");
  await closeProbe(page, 2);
  await open.click({ trial: true });
  const first = bridge.callsFor("confirm_window_close").at(-1).report;
  assert.equal(first.ticket, 2);
  assert.equal(first.hasDraft, false);
  assert.deepEqual(bridge.callsFor("complete_window_close"), [
    { ticket: 2, draftGeneration: first.draftGeneration, proceed: true },
  ]);

  const field = page.getByRole("table").getByRole("textbox");
  await field.fill("9,007,199,254,740,993");
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .waitFor();
  bridge.chooseClose("cancel");
  await closeProbe(page, 3);
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click({ trial: true });
  const second = bridge.callsFor("confirm_window_close").at(-1).report;
  assert.equal(second.ticket, 3);
  assert(second.draftGeneration > first.draftGeneration);
  assert.equal(second.hasDraft, true);
  assert.equal(await field.inputValue(), "9,007,199,254,740,993");
  assert.equal(bridge.callsFor("complete_window_close").length, 1);
}
