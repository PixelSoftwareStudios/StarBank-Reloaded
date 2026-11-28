import assert from "node:assert/strict";

const openingXml =
  '\uFEFF<Bank version="1">\r\n  <Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section>\r\n</Bank>\r\n';
const preparedXml = openingXml.replace('int="001"', 'int="002"');

export const clipboardBank = Buffer.from(openingXml);

export async function configureDesktopClipboard(page, mode) {
  await page.addInitScript((mode) => {
    window.fixtureClipboardWrites = [];
    window.fixtureClipboardAllowed = false;
    Object.defineProperty(navigator, "clipboard", {
      value:
        mode === "absent"
          ? undefined
          : {
              async writeText(text) {
                window.fixtureClipboardWrites.push(text);
                if (!window.fixtureClipboardAllowed) {
                  throw new DOMException("Clipboard denied", "NotAllowedError");
                }
              },
            },
    });
  }, mode);
}

async function copyBank(page) {
  await page
    .getByRole("button", { name: "More save options", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "Copy bank XML to clipboard", exact: true })
    .click();
}

export async function verifyDesktopClipboard(page, bridge, mode) {
  const active = bridge.wireRequests.findLast(
    ({ command }) => command.kind === "raw_text",
  );
  assert(active);
  const start = bridge.wireRequests.length;
  const operations = () =>
    bridge.wireRequests.slice(start).map(({ command }) => command.kind);
  await copyBank(page);

  const dialog = page.getByRole("dialog", {
    name: "Copy bank XML",
    exact: true,
  });
  const field = dialog.getByRole("textbox", {
    name: "Prepared bank XML",
    exact: true,
  });
  await field.waitFor();
  assert.equal(await field.inputValue(), preparedXml.replaceAll("\r\n", "\n"));
  assert.equal(await field.getAttribute("readonly"), "");
  const manual = await field.evaluate((textarea) => {
    textarea.select();
    const clipboardData = new DataTransfer();
    const handled = !textarea.dispatchEvent(
      new ClipboardEvent("copy", {
        bubbles: true,
        cancelable: true,
        clipboardData,
      }),
    );
    return { handled, xml: clipboardData.getData("text/plain") };
  });
  assert.deepEqual(manual, { handled: true, xml: preparedXml });
  assert.deepEqual(operations(), ["prepare"]);
  const prepare = bridge.wireRequests.at(-1);
  assert.equal(prepare.document_id, active.document_id);
  assert.equal(prepare.expected_revision, active.expected_revision);

  const retry = dialog.getByRole("button", { name: "Copy XML", exact: true });
  if (mode === "absent") {
    assert.equal(await retry.count(), 0);
    assert.deepEqual(
      await page.evaluate(() => window.fixtureClipboardWrites),
      [],
    );
  } else {
    assert.deepEqual(await page.evaluate(() => window.fixtureClipboardWrites), [
      preparedXml,
    ]);
    await page.evaluate(() => {
      window.fixtureClipboardAllowed = true;
    });
    await retry.click();
    await dialog
      .getByRole("status")
      .filter({ hasText: "Bank XML copied." })
      .waitFor();
    assert.deepEqual(await page.evaluate(() => window.fixtureClipboardWrites), [
      preparedXml,
      preparedXml,
    ]);
    assert.deepEqual(operations(), ["prepare"]);
  }

  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  await page
    .locator("dialog")
    .filter({ hasText: "Copy bank XML" })
    .waitFor({ state: "detached" });
  if (mode === "denied") {
    await copyBank(page);
    await page
      .getByRole("status")
      .filter({ hasText: "Copied bank XML." })
      .waitFor();
    assert.equal(await dialog.count(), 0);
    assert.deepEqual(await page.evaluate(() => window.fixtureClipboardWrites), [
      preparedXml,
      preparedXml,
      preparedXml,
    ]);
    assert.deepEqual(operations(), ["prepare", "prepare"]);
  }
  await page
    .getByRole("status")
    .filter({ hasText: "Unsaved changes in 1 section" })
    .waitFor();
  const snapshot = bridge.dispatchFixture({
    schema_version: 1,
    request_id: "clipboard-fixture-snapshot",
    document_id: active.document_id,
    expected_revision: active.expected_revision,
    command: { kind: "snapshot" },
  });
  assert.equal(snapshot.revision, active.expected_revision);
  assert.equal(snapshot.result.kind, "snapshot");
  assert.equal(snapshot.result.payload.dirty, true);
  for (const command of [
    "save_active_bank",
    "save_bank_as",
    "export_bank_copy",
  ])
    assert.deepEqual(bridge.callsFor(command), []);
}
