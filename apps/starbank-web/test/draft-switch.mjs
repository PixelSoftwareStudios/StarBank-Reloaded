import assert from "node:assert/strict";

const bank =
  '<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key><Key name="Rank"><Value int="002"/></Key></Section></Bank>';

export async function verifyTableDraftSwitch(page, chooseBank) {
  await page.getByRole("button", { name: "Open Bank…" }).waitFor();
  await chooseBank("DraftFixture.SC2Bank", bank);
  const tableTab = page.getByRole("tab", { name: "Table View" });
  const rawTab = page.getByRole("tab", { name: "Raw View" });
  const raw = page.getByRole("textbox", { name: "Bank XML" });
  const gold = page.getByRole("textbox", { name: "Gold · Value value" });
  const rank = page.getByRole("textbox", { name: "Rank · Value value" });
  const apply = page.getByRole("button", { name: "Apply edits" });
  const undo = page.getByRole("button", { name: "Undo", exact: true });
  const dirty = page.getByRole("status").filter({ hasText: "Unsaved changes" });
  const decision = page.getByRole("dialog", {
    name: "Table draft could not be applied",
  });

  await gold.fill("1,234");
  await rank.fill("5,678");
  await rawTab.press("Enter");
  await raw.waitFor();
  const applied = await raw.textContent();
  assert(applied.includes('int="1234"'), applied);
  assert(applied.includes('int="5678"'), applied);
  await dirty.waitFor();
  await apply.waitFor({ state: "hidden" });
  await tableTab.click();
  await undo.click();
  await dirty.waitFor({ state: "hidden" });
  assert.equal(await gold.inputValue(), "001");
  assert.equal(await rank.inputValue(), "002");

  await gold.fill("9,999");
  await rank.fill("NaN");
  assert.equal(await apply.isEnabled(), true);
  await rawTab.click();
  await decision
    .getByText("Enter an integer with optional comma grouping.")
    .waitFor();
  await decision.getByRole("button", { name: "Cancel", exact: true }).click();
  await decision.waitFor({ state: "hidden" });
  assert.equal(await tableTab.getAttribute("aria-selected"), "true");
  assert.equal(await gold.inputValue(), "9,999");
  assert.equal(await rank.inputValue(), "NaN");
  assert.equal(await dirty.count(), 0);

  await rank.fill("12,34");
  assert.equal(await apply.isDisabled(), true);
  await tableTab.press("ArrowRight");
  assert.equal(await rawTab.getAttribute("aria-selected"), "false");
  await rawTab.press("Enter");
  await decision.getByRole("button", { name: "Repair edits" }).click();
  await decision.waitFor({ state: "hidden" });
  assert.equal(await tableTab.getAttribute("aria-selected"), "true");
  assert.equal(await rank.inputValue(), "12,34");
  await page.waitForFunction(
    (label) => document.activeElement?.getAttribute("aria-label") === label,
    await rank.getAttribute("aria-label"),
  );

  await rawTab.press("Space");
  await decision.getByRole("button", { name: "Discard and switch" }).click();
  await raw.waitFor();
  assert((await raw.textContent()).includes(bank));
  assert.equal(await dirty.count(), 0);
  await tableTab.click();
  assert.equal(await gold.inputValue(), "001");
  assert.equal(await rank.inputValue(), "002");
}
