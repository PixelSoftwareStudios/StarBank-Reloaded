import assert from "node:assert/strict";

const syntaxHint = "Possible XML syntax issue";

export async function verifyRawSyntaxDiagnostics(page, chooseBank) {
  const rawTab = page.getByRole("tab", { name: "Raw View" });
  const raw = page.getByLabel("Bank XML", { exact: true });
  const issue = raw.locator(`[title="${syntaxHint}"]`);
  const apply = page.getByRole("button", { name: "Apply XML" });
  const draft =
    '<Bank><Section name="Fixture"><Key name="Score"><Value int="1"/></Key></Section><!-- preserve this draft -->';
  const repaired = `${draft}</Bank>`;

  await chooseBank("Raw diagnostics.SC2Bank", "<Bank/>");
  await page
    .getByRole("heading", { name: "Raw diagnostics.SC2Bank" })
    .waitFor();
  await rawTab.click();
  await raw.fill("<Bank @/>");
  const rangedIssue = issue.filter({ hasText: "@" });
  await rangedIssue.waitFor();
  assert.deepEqual(
    await rangedIssue.evaluate((element) => {
      const style = getComputedStyle(element);
      return [style.textDecorationLine, style.textDecorationStyle];
    }),
    ["underline", "wavy"],
  );
  await raw.fill(draft);
  await issue.first().waitFor();
  assert.notEqual(
    await issue
      .first()
      .evaluate((element) => getComputedStyle(element).boxShadow),
    "none",
  );
  assert.equal(await raw.getAttribute("aria-description"), syntaxHint);
  assert.equal(await raw.textContent(), draft);
  assert.equal(await apply.isEnabled(), true);

  await apply.click();
  await page
    .getByRole("status")
    .filter({ hasText: "The bank XML could not be opened safely." })
    .first()
    .waitFor();
  assert.equal(await raw.textContent(), draft);

  await raw.press("End");
  await page.keyboard.insertText("</Bank>");
  await issue.first().waitFor({ state: "detached" });
  assert.equal(await raw.getAttribute("aria-description"), null);
  assert.equal(await raw.textContent(), repaired);
  await apply.click();
  await apply.waitFor({ state: "hidden" });
  assert.equal(await raw.textContent(), repaired);

  await raw.fill(draft);
  await page
    .getByRole("button", { name: "Discard draft", exact: true })
    .click();
  await chooseBank("Other raw bank.SC2Bank", "<Bank/>");
  await page
    .getByRole("dialog", { name: "Unsaved bank changes" })
    .getByRole("button", { name: "Discard and open" })
    .click();
  await page.getByRole("heading", { name: "Other raw bank.SC2Bank" }).waitFor();
  await rawTab.click();
  await raw.waitFor();
  // Let any retired editor's debounce elapse before inspecting the replacement.
  await page.waitForTimeout(350);
  assert.equal(await raw.textContent(), "<Bank/>");
  assert.equal(await issue.count(), 0);
  assert.equal(await raw.getAttribute("aria-description"), null);
}
