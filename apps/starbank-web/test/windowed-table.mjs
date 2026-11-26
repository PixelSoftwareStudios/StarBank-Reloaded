import assert from "node:assert/strict";

const bankRows = 10_000;
const distantIndex = 9_000;
const rangeEndIndex = 200;
const mountedRowLimit = 80;
const exactValue = "9007199254740993";
const groupedValue = "9,007,199,254,740,993";

function syntheticBank() {
  const keys = Array.from({ length: bankRows }, (_, index) => {
    const value = index === 0 ? "001" : String(index);
    return `<Key name="Row ${index}"><Value int="${value}"/></Key>`;
  }).join("");
  const small =
    '<Key name="Small field"><Value fixed="2.50"/></Key><Key name="Opaque"><Value future="kept"/></Key><Key name="Last field"><Value fixed="4.50"/></Key>';
  return `<Bank><Section name="Large">${keys}</Section><Section name="Small">${small}</Section></Bank>`;
}

export async function verifyWindowedTable(page, chooseBank) {
  await page.getByRole("button", { name: "Open Bank…" }).waitFor();
  await chooseBank("WindowFixture.SC2Bank", syntheticBank());
  const table = page.getByRole("table", { name: "Large", exact: true });
  const viewport = page.locator(".editor-windowed-table");
  const dataRows = table.locator('tbody tr:not([aria-hidden="true"])');
  const countWindow = async () => {
    const mounted = await table.locator("tbody tr").count();
    assert(mounted < mountedRowLimit, `Mounted ${mounted} table body rows`);
    return dataRows.count();
  };
  const fieldName = (index) => `Row ${index} · Value value`;
  const field = (index) =>
    table.getByRole("textbox", { name: fieldName(index), exact: true });
  const waitForValue = (index, value) =>
    page.waitForFunction(
      ({ label, value }) =>
        Array.from(document.querySelectorAll("input[aria-label]")).some(
          (input) =>
            input.getAttribute("aria-label") === label && input.value === value,
        ),
      { label: fieldName(index), value },
    );
  const selection = (index) =>
    table.getByRole("checkbox", {
      name: `Select Row ${index} · Value`,
      exact: true,
    });
  const scrollTo = (index) =>
    viewport.evaluate((scroll, target) => {
      const row = scroll.querySelector("tbody tr[data-bank-row-index]");
      scroll.scrollTop = target * row.getBoundingClientRect().height;
    }, index);

  const first = field(0);
  await first.waitFor();
  const initialRows = await countWindow();
  assert(initialRows > 1);
  assert.equal(await table.getAttribute("aria-rowcount"), String(bankRows + 1));
  assert.equal(await table.getByRole("row").count(), initialRows + 1);
  assert.equal(await table.getByRole("columnheader").count(), 3);
  assert.equal(await first.inputValue(), "001");

  await selection(0).check();
  await scrollTo(rangeEndIndex);
  const rangeEnd = selection(rangeEndIndex);
  await rangeEnd.press("Shift+Space");
  const rangeEditor = page.getByLabel(
    `Set value for ${rangeEndIndex + 1} rows`,
    { exact: true },
  );
  await rangeEditor.waitFor();
  assert.equal(await rangeEnd.isChecked(), true);
  assert.equal(await selection(rangeEndIndex + 1).isChecked(), false);
  await rangeEnd.press("Shift+Space");
  await rangeEditor.waitFor({ state: "hidden" });
  assert.equal(await rangeEnd.isChecked(), false);
  await scrollTo(0);
  await selection(0).waitFor();
  assert.equal(await selection(0).isChecked(), false);

  await first.fill(exactValue);
  await scrollTo(distantIndex);
  const distant = field(distantIndex);
  await distant.waitFor();
  const scrolledRows = await countWindow();
  assert.equal(
    await first.evaluate((input) => input === document.activeElement),
    true,
  );
  assert.equal(await first.inputValue(), exactValue);
  assert.equal(
    await distant.evaluate((input) =>
      input.closest("tr").getAttribute("aria-rowindex"),
    ),
    String(distantIndex + 2),
  );

  await page.keyboard.press("Tab");
  const rowOne = selection(1);
  const rowOneLabel = await rowOne.getAttribute("aria-label");
  await page.waitForFunction(
    (label) => document.activeElement?.getAttribute("aria-label") === label,
    rowOneLabel,
  );
  await rowOne.check();
  await scrollTo(distantIndex);
  await selection(distantIndex).check();
  await rowOne.waitFor({ state: "detached" });

  await page
    .getByLabel("Set value for 2 rows", { exact: true })
    .fill(groupedValue);
  await page.getByRole("button", { name: "Stage values", exact: true }).click();
  await page.getByRole("button", { name: "Apply edits", exact: true }).click();
  const dirty = page.getByRole("status").filter({ hasText: "Unsaved changes" });
  await dirty.waitFor();
  assert.equal(await distant.inputValue(), groupedValue);
  assert.equal(await field(distantIndex + 1).inputValue(), "9,001");
  await scrollTo(0);
  await field(1).waitFor();
  assert.equal(await field(1).inputValue(), groupedValue);
  assert.equal(await first.inputValue(), groupedValue);

  const undo = page.getByRole("button", { name: "Undo", exact: true });
  await undo.click();
  await waitForValue(0, "001");
  await waitForValue(1, "1");
  await dirty.waitFor({ state: "hidden" });
  await scrollTo(distantIndex);
  await waitForValue(distantIndex, "9,000");

  await viewport.evaluate((scroll) => {
    scroll.scrollTop = scroll.scrollHeight;
  });
  const last = field(bankRows - 1);
  await last.waitFor();
  assert.equal(await last.inputValue(), "9,999");
  const next = page.getByRole("button", { name: "Next rows", exact: true });
  assert.equal(await next.isDisabled(), true);
  await page
    .getByRole("button", { name: "Previous rows", exact: true })
    .click();
  assert.equal(await next.isEnabled(), true);

  const filter = page.getByRole("searchbox", { name: "Filter rows by name" });
  await filter.fill("rOw 999");
  await field(999).waitFor();
  assert.equal(await countWindow(), 11);
  const sort = table.getByRole("button", { name: "Name", exact: true });
  await sort.click();
  await sort.click();
  assert.equal(
    await dataRows.first().getByRole("textbox").getAttribute("aria-label"),
    fieldName(9999),
  );
  await last.focus();
  assert.equal(await last.inputValue(), "9999");
  await last.fill("10001");
  await filter.fill("Row 100");
  await page.getByText("The row filter hides some staged edits.").waitFor();
  await filter.fill("Row 999");
  assert.equal(await last.inputValue(), "10001");
  await page.getByRole("button", { name: "Discard draft" }).click();
  assert.equal(await last.inputValue(), "9,999");
  await sort.click();
  await filter.fill("");

  await page
    .getByRole("navigation", { name: "Bank sections" })
    .getByRole("button", { name: "Small", exact: true })
    .click();
  const small = page.getByRole("table", { name: "Small", exact: true });
  assert.equal(await small.getAttribute("aria-rowcount"), null);
  assert.equal(
    await small
      .getByRole("textbox", { name: "Small field · Value value" })
      .inputValue(),
    "2.50",
  );

  const firstSmall = small.getByRole("checkbox", {
    name: "Select Small field · Value",
    exact: true,
  });
  const lastSmall = small.getByRole("checkbox", {
    name: "Select Last field · Value",
    exact: true,
  });
  const untouched = small.getByText("kept", { exact: true });
  await firstSmall.check();
  await lastSmall.click({ modifiers: ["Shift"] });
  const smallEdit = page.getByLabel("Set value for 2 rows", { exact: true });
  await smallEdit.waitFor();
  assert.equal(await smallEdit.getAttribute("placeholder"), "Mixed values");
  await untouched.waitFor();
  await smallEdit.fill("-1,234.50");
  await page.getByRole("button", { name: "Stage values", exact: true }).click();
  await page.getByRole("button", { name: "Apply edits", exact: true }).click();
  await dirty.waitFor();
  const smallFields = small.getByRole("textbox");
  assert.equal(await smallFields.first().inputValue(), "-1,234.50");
  assert.equal(await smallFields.last().inputValue(), "-1,234.50");
  await untouched.waitFor();
  await undo.click();
  await dirty.waitFor({ state: "hidden" });
  assert.equal(await smallFields.first().inputValue(), "2.50");
  assert.equal(await smallFields.last().inputValue(), "4.50");

  return { initialRows, scrolledRows };
}
