import assert from "node:assert/strict";

const bank =
  '<Bank><Section name="Player">' +
  '<Key name="Gold"><Value int="1"/><Value int="1"/></Key>' +
  '<Key name="Level"><Value int="1"/></Key>' +
  "</Section></Bank>";

export async function verifyDiffContext(browser, url, bankPicker) {
  const page = await browser.newPage({ viewport: { width: 900, height: 700 } });
  try {
    await page.goto(url);
    await bankPicker(page)("DiffContext.SC2Bank", bank);
    const gold = page.getByRole("textbox", { name: "Gold · Value value" });
    const level = page.getByRole("textbox", { name: "Level · Value value" });
    await gold.first().waitFor();
    assert.equal(await gold.count(), 2);
    const fields = [gold.first(), gold.last(), level];
    const values = () => Promise.all(fields.map((field) => field.inputValue()));

    for (const field of fields) await field.fill("2");
    await page.getByRole("button", { name: "Apply edits" }).click();
    const changes = page.getByRole("button", { name: "Changes…" });
    const diff = page.getByRole("dialog", { name: "Changes since opening" });
    const undo = page.getByRole("button", { name: "Undo", exact: true });

    await changes.click();
    for (const path of [
      "Player › Gold › Value (1 of 2)",
      "Player › Gold › Value (2 of 2)",
      "Player › Level › Value",
    ]) {
      await diff.getByText(`Changed value ${path}`, { exact: true }).waitFor();
      assert.equal(
        await diff
          .getByRole("button", { name: `Revert value ${path}`, exact: true })
          .count(),
        1,
      );
    }

    await diff
      .getByRole("button", {
        name: "Revert value Player › Gold › Value (2 of 2)",
        exact: true,
      })
      .click();
    await diff.waitFor({ state: "hidden" });
    assert.deepEqual(await values(), ["2", "1", "2"]);
    await undo.click();
    await changes.click();
    assert.deepEqual(await values(), ["2", "2", "2"]);

    await diff
      .getByRole("button", {
        name: "Revert value Player › Level › Value",
        exact: true,
      })
      .click();
    await diff.waitFor({ state: "hidden" });
    assert.deepEqual(await values(), ["2", "2", "1"]);
    await undo.click();
    await changes.click();
    assert.deepEqual(await values(), ["2", "2", "2"]);
  } finally {
    await page.close();
  }
}
