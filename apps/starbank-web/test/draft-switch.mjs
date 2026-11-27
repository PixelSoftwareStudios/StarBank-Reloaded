import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bank =
  '<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key><Key name="Rank"><Value int="002"/></Key><Key name="Precise"><Value fixed="0.001"/></Key></Section></Bank>';

export async function verifyTableDraftSwitch(page, chooseBank) {
  await page.getByRole("button", { name: "Open Bank…" }).waitFor();
  await chooseBank("DraftFixture.SC2Bank", bank);
  const tableTab = page.getByRole("tab", { name: "Table View" });
  const rawTab = page.getByRole("tab", { name: "Raw View" });
  const raw = page.getByRole("textbox", { name: "Bank XML" });
  const gold = page.getByRole("textbox", { name: "Gold · Value value" });
  const rank = page.getByRole("textbox", { name: "Rank · Value value" });
  const precise = page.getByRole("textbox", { name: "Precise · Value value" });
  const apply = page.getByRole("button", { name: "Apply edits" });
  const undo = page.getByRole("button", { name: "Undo", exact: true });
  const dirty = page.getByRole("status").filter({ hasText: "Unsaved changes" });
  const decision = page.getByRole("dialog", {
    name: "Table draft could not be applied",
  });

  async function holdInvalidDraft() {
    await page.evaluate(() =>
      window.dispatchEvent(new Event("starbank-test-hold-validation")),
    );
    await rank.fill("NaN");
    await page.waitForFunction(
      () => typeof window.releaseFixtureValidation === "function",
    );
    assert.equal(await apply.isDisabled(), true);
  }

  async function discardHeldDraft() {
    await holdInvalidDraft();
    await page
      .getByRole("button", { name: "Discard draft", exact: true })
      .click();
    await apply.waitFor({ state: "hidden" });
  }

  async function waitForHeldAction() {
    const status = page.getByRole("status").filter({
      hasText: /Working…|already has a command in progress/u,
    });
    await status.waitFor();
    assert.match(await status.textContent(), /^Working…/u);
  }

  const releaseValidation = () =>
    page.evaluate(() => window.releaseFixtureValidation());

  await holdInvalidDraft();
  await gold.fill("9,007,199,254,740,993");
  await rank.fill("5,678");
  await rawTab.press("Enter");
  assert.equal(await tableTab.getAttribute("aria-selected"), "true");
  await page.evaluate(() => {
    window.fixtureSawStaleIssue = false;
    const observer = new MutationObserver(() => {
      if (document.querySelector('input[aria-invalid="true"]'))
        window.fixtureSawStaleIssue = true;
    });
    observer.observe(document.body, { subtree: true, attributes: true });
    window.releaseFixtureValidation();
  });
  await raw.waitFor();
  const applied = await raw.textContent();
  assert(applied.includes('int="9007199254740993"'), applied);
  assert.equal(await page.evaluate(() => window.fixtureSawStaleIssue), false);
  assert(applied.includes('int="5678"'), applied);
  await dirty.waitFor();
  await apply.waitFor({ state: "hidden" });
  await tableTab.click();
  await discardHeldDraft();
  await undo.click();
  await waitForHeldAction();
  await releaseValidation();
  await dirty.waitFor({ state: "hidden" });
  assert.equal(await gold.inputValue(), "001");
  assert.equal(await rank.inputValue(), "002");

  await gold.fill("9,999");
  await rank.fill("NaN");
  const invalid = page.locator('input[aria-invalid="true"]');
  await rank.and(invalid).waitFor();
  const rowIssue = await rank.getAttribute("aria-describedby");
  assert.equal(
    await page.locator(`[id="${rowIssue}"]`).textContent(),
    "Enter an integer with optional comma grouping.",
  );
  assert.equal(await apply.isDisabled(), true);
  assert.equal(await gold.getAttribute("aria-invalid"), null);
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

  await rank.fill("2.5");
  await rank.and(invalid).waitFor();
  await precise.fill("1e3");
  await precise.and(invalid).waitFor();
  const fixedIssue = await precise.getAttribute("aria-describedby");
  assert.equal(
    await page.locator(`[id="${fixedIssue}"]`).textContent(),
    "Enter an exact decimal with optional comma grouping.",
  );
  await precise.fill("0.001");
  await rank.fill("12,34");
  await rank.and(invalid).waitFor();
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

  await discardHeldDraft();
  await page.getByRole("button", { name: "Save As…", exact: true }).click();
  const save = page.getByRole("dialog", { name: "Save a bank copy" });
  await save.getByLabel("Re-sign this copy").uncheck();
  await save.getByRole("button", { name: "Prepare download" }).click();
  const preparing = save.getByRole("button", { name: "Preparing…" });
  await preparing.or(save.getByRole("alert")).waitFor();
  assert.equal(await preparing.isDisabled(), true);
  await releaseValidation();
  const download = page.waitForEvent("download");
  await save
    .getByRole("link", { name: "Download DraftFixture.SC2Bank" })
    .click();
  assert.deepEqual(
    await readFile(await (await download).path()),
    Buffer.from(bank),
  );
  await save.getByRole("button", { name: "Close", exact: true }).click();

  await discardHeldDraft();
  await chooseBank(
    "ReplacementDraftFixture.SC2Bank",
    bank.replace('int="001"', 'int="003"'),
  );
  await waitForHeldAction();
  await releaseValidation();
  await page
    .getByRole("heading", { name: "ReplacementDraftFixture.SC2Bank" })
    .waitFor();
  assert.equal(await gold.inputValue(), "003");
}
