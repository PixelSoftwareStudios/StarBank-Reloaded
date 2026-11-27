import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bank =
  '<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key><Key name="Rank"><Value int="002"/></Key><Key name="Precise"><Value fixed="0.001"/></Key><Key name="Caption"><Value string="Original"/></Key></Section></Bank>';

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
  assert.equal(
    await page
      .getByRole("button", { name: "Save As…", exact: true })
      .isDisabled(),
    true,
  );
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
  assert.equal(await invalid.count(), 0);

  await verifyXmlScalarDraft(page);
  await verifyRepairFocus(page, chooseBank);
}

async function verifyXmlScalarDraft(page) {
  const caption = page.getByRole("textbox", {
    name: "Caption · Value value",
    exact: true,
  });
  const apply = page.getByRole("button", { name: "Apply edits", exact: true });
  const save = page.getByRole("button", { name: "Save As…", exact: true });
  const dirty = page.getByRole("status").filter({ hasText: "Unsaved changes" });
  const invalid = caption.and(page.locator('input[aria-invalid="true"]'));
  // Native text insertion retains this XML-forbidden scalar in all three engines.
  const illegal = "left\uFFFEright";

  await caption.fill(illegal);
  assert.equal(await caption.inputValue(), illegal);
  await invalid.waitFor();
  const issue = await caption.getAttribute("aria-describedby");
  assert.equal(
    await page.locator(`[id="${issue}"]`).textContent(),
    "This value contains a character XML cannot store.",
  );
  assert.equal(await apply.isDisabled(), true);
  assert.equal(await save.isDisabled(), true);
  assert.equal(await dirty.count(), 0);

  const legal = "a & b < c";
  await caption.fill(legal);
  await apply.click();
  await apply.waitFor({ state: "hidden" });
  await dirty.waitFor();
  assert.equal(await invalid.count(), 0);
  assert.equal(await caption.inputValue(), legal);

  await page.getByRole("tab", { name: "Raw View" }).click();
  const raw = page.getByRole("textbox", { name: "Bank XML" });
  await raw.waitFor();
  assert((await raw.textContent()).includes('string="a &amp; b &lt; c"'));
  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await dirty.waitFor({ state: "hidden" });
  await page.getByRole("tab", { name: "Table View" }).click();
  assert.equal(await caption.inputValue(), "Original");
}

async function verifyRepairFocus(page, chooseBank) {
  const rowCount = 180;
  const rowName = (index) => `Amount ${String(index).padStart(3, "0")}`;
  const keys = Array.from(
    { length: rowCount },
    (_, index) => `<Key name="${rowName(index)}"><Value int="${index}"/></Key>`,
  ).join("");
  await chooseBank(
    "RepairFixture.SC2Bank",
    `<Bank><Section name="Inventory">${keys}</Section><Section name="Other"><Key name="Visible valid"><Value int="7"/></Key></Section></Bank>`,
  );
  const inventory = page.getByRole("table", { name: "Inventory", exact: true });
  const field = (index) =>
    inventory.getByRole("textbox", {
      name: `${rowName(index)} · Value value`,
      exact: true,
    });
  const selection = (index) =>
    inventory.getByRole("checkbox", {
      name: `Select ${rowName(index)} · Value`,
      exact: true,
    });
  const section = (name) =>
    page
      .getByRole("navigation", { name: "Bank sections" })
      .getByRole("button", { name, exact: true });
  const viewport = page.locator(".editor-windowed-table");
  const invalid = page.locator('input[aria-invalid="true"]');
  const rowFilter = page.getByRole("searchbox", {
    name: "Filter rows by name",
  });
  const sectionFilter = page.getByRole("searchbox", {
    name: "Filter sections by name",
  });
  const apply = page.getByRole("button", { name: "Apply edits", exact: true });
  const rawTab = page.getByRole("tab", { name: "Raw View" });
  const decision = page.getByRole("dialog", {
    name: "Table draft could not be applied",
  });
  const last = rowCount - 1;

  const scrollTo = (index) =>
    viewport.evaluate((scroll, offset) => {
      const sample = scroll.querySelector("[data-bank-row-index]");
      scroll.scrollTop = offset * sample.getBoundingClientRect().height;
    }, index);

  async function repair() {
    await rawTab.press("Enter");
    await decision
      .getByRole("button", { name: "Repair edits", exact: true })
      .click();
    await page.waitForFunction(
      (label) => document.activeElement?.getAttribute("aria-label") === label,
      `${rowName(last)} · Value value`,
    );
    assert.equal(await field(last).inputValue(), "12,34");
    assert.equal(await apply.isDisabled(), true);
  }

  await field(0).waitFor();
  await scrollTo(last);
  await selection(last).check();
  await scrollTo(0);
  await selection(0).check();
  await page.getByLabel("Set value for 2 rows", { exact: true }).fill("12,34");
  await page.getByRole("button", { name: "Stage values", exact: true }).click();
  await field(0).and(invalid).waitFor();
  const nameSort = inventory.getByRole("button", { name: "Name", exact: true });
  await nameSort.click();
  await nameSort.click();
  await scrollTo(last);
  await field(0).and(invalid).waitFor();
  assert.equal(await field(last).count(), 0);
  await repair();
  assert.equal(
    await inventory
      .getByRole("columnheader", { name: "Name" })
      .getAttribute("aria-sort"),
    "descending",
  );
  assert.equal(await selection(last).isChecked(), true);
  const rowBounds = await field(last).evaluate((input) => {
    const bounds = input.getBoundingClientRect();
    const scroll = input.closest(".editor-windowed-table");
    return {
      top: bounds.top,
      bottom: bounds.bottom,
      heading: scroll.querySelector("thead").getBoundingClientRect().bottom,
      viewport: scroll.getBoundingClientRect().bottom,
      offset: scroll.scrollTop,
      size: scroll.clientHeight,
    };
  });
  const middle = (rowBounds.top + rowBounds.bottom) / 2;
  assert(
    middle >= rowBounds.heading && middle <= rowBounds.viewport,
    JSON.stringify(rowBounds),
  );
  if (
    rowBounds.viewport - rowBounds.heading >=
    rowBounds.bottom - rowBounds.top
  ) {
    assert(
      rowBounds.top >= rowBounds.heading &&
        rowBounds.bottom <= rowBounds.viewport,
      JSON.stringify(rowBounds),
    );
  }
  await scrollTo(last);
  await selection(0).waitFor();
  assert.equal(await selection(0).isChecked(), true);
  assert.equal(await field(0).inputValue(), "12,34");
  assert.equal(
    await page.getByLabel("Set value for 2 rows", { exact: true }).inputValue(),
    "12,34",
  );

  await rowFilter.fill("Amount 060");
  assert.equal(await invalid.count(), 0);
  await repair();
  assert.equal(await rowFilter.inputValue(), "Amount 060");
  await field(60).waitFor();

  await section("Other").click();
  await rowFilter.fill("Visible valid");
  await sectionFilter.fill("Other");
  assert.equal(await section("Inventory").count(), 0);
  const other = page.getByRole("textbox", {
    name: "Visible valid · Value value",
    exact: true,
  });
  await other.fill("0017");
  await repair();
  assert.equal(await section("Inventory").getAttribute("aria-current"), "true");
  assert.equal(await sectionFilter.inputValue(), "Other");
  assert.equal(await rowFilter.inputValue(), "Visible valid");
  assert.equal(
    await inventory
      .getByRole("columnheader", { name: "Name" })
      .getAttribute("aria-sort"),
    "descending",
  );
  await rowFilter.fill("Amount 060");
  assert.equal(await field(last).count(), 0);
  await section("Other").click();
  await rowFilter.fill("Visible valid");
  assert.equal(await other.inputValue(), "0017");
}
