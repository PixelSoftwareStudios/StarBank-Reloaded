import assert from "node:assert/strict";

export async function verifyCreateOnlyClose(
  page,
  bridge,
  { bankName, savedName, value, refusal },
) {
  await page.waitForFunction(
    () => typeof window.fixtureCloseProbe === "function",
  );
  const heading = (name) => page.getByRole("heading", { name, exact: true });
  const field = page.getByRole("table").getByRole("textbox");
  const dirty = page.locator(".editor-dirty-status");
  const open = page.getByRole("button", { name: "Open Bank…", exact: true });
  const saveCount = bridge.callsFor("save_bank_as").length;
  const openCount = bridge.callsFor("open_bank").length;
  const active = bridge.wireRequests.findLast(
    ({ command }) => command.kind === "raw_text",
  );
  assert(active);
  await dirty.waitFor();

  for (const [ticket, outcome] of [
    [1, "cancel"],
    [2, "fail"],
    [3, "success"],
  ]) {
    const saved = outcome === "success";
    bridge.chooseClose("save");
    bridge.choose("save", outcome);
    await page.evaluate(
      (ticket) => window.fixtureCloseProbe({ ticket }),
      ticket,
    );
    await page.waitForFunction(
      (ticket) => window.fixtureCompletedCloseTicket === ticket,
      ticket,
    );
    await open.click({ trial: true });

    const { report } = bridge.callsFor("confirm_window_close").at(-1);
    assert.equal(report.ticket, ticket);
    assert.equal(report.hasDraft, false);
    assert.equal(report.canSave, true);
    assert.deepEqual(bridge.callsFor("complete_window_close").at(-1), {
      ticket,
      draftGeneration: report.draftGeneration,
      proceed: saved,
    });
    const saves = bridge.callsFor("save_bank_as").slice(saveCount);
    assert.equal(saves.length, ticket);
    assert.equal(saves.at(-1).target.documentId, active.document_id);
    assert.equal(saves.at(-1).target.revision, active.expected_revision);
    assert.equal(bridge.callsFor("open_bank").length, openCount);
    assert.equal(await field.inputValue(), value);

    await heading(saved ? savedName : bankName).waitFor();
    await dirty.waitFor({ state: saved ? "detached" : "visible" });
    if (outcome === "fail") {
      await page.getByRole("status").filter({ hasText: refusal }).waitFor();
    }
  }
}
