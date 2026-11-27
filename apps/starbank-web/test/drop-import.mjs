import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const openingName = "Dropped.SC2Bank";
const openingBytes = Buffer.from(
  '\uFEFF<?xml version="1.0" encoding="UTF-16"?>\r\n<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section></Bank>\r\n',
  "utf16le",
);
const replacementName = "Dropped replacement.SC2Bank";
const replacementBytes = Buffer.from(
  '<Bank><Section name="Player"><Key name="Gold"><Value int="003"/></Key></Section></Bank>',
);

async function dropFiles(surface, files) {
  const prevented = await surface.evaluate(
    (element, files) => {
      const transfer = new DataTransfer();
      for (const { name, bytes } of files) {
        transfer.items.add(
          new File([Uint8Array.from(bytes)], name, {
            type: "application/octet-stream",
          }),
        );
      }
      return ["dragover", "drop"].map(
        (type) =>
          !element.dispatchEvent(
            new DragEvent(type, {
              bubbles: true,
              cancelable: true,
              dataTransfer: transfer,
            }),
          ),
      );
    },
    files.map(([name, bytes]) => ({ name, bytes: Array.from(bytes) })),
  );
  assert.deepEqual(prevented, [true, true]);
}

export async function verifyDropImport(context, url, viewport) {
  const page = await context.newPage();
  try {
    await page.addInitScript(() => {
      window.fixtureDropOpens = 0;
      const BrowserWorker = globalThis.Worker;
      globalThis.Worker = class extends BrowserWorker {
        postMessage(request, transfer) {
          if (
            typeof request?.control === "string" &&
            JSON.parse(request.control).command.kind === "open"
          )
            window.fixtureDropOpens += 1;
          super.postMessage(request, transfer);
        }
      };
    });
    await page.setViewportSize(viewport);
    await page.goto(url);
    await page
      .getByRole("button", { name: "Open Bank…", exact: true })
      .waitFor();
    const openCount = () => page.evaluate(() => window.fixtureDropOpens);
    const heading = (name) => page.getByRole("heading", { name, exact: true });
    const status = (text) => page.getByRole("status").filter({ hasText: text });
    const value = page.getByRole("textbox", {
      name: "Gold · Value value",
      exact: true,
    });
    const editor = page.locator(".document-editor");
    const replacement = [replacementName, replacementBytes];

    await dropFiles(page.locator(".editor-welcome-frame"), [
      [openingName, openingBytes],
    ]);
    await heading(openingName).waitFor();
    assert.equal(await openCount(), 1);
    assert.equal(await value.inputValue(), "001");

    await page.getByRole("button", { name: "Save As…", exact: true }).click();
    const save = page.getByRole("dialog", {
      name: "Save a bank copy",
      exact: true,
    });
    await save.getByLabel("Re-sign this copy", { exact: true }).uncheck();
    await save
      .getByRole("button", { name: "Prepare download", exact: true })
      .click();
    const download = page.waitForEvent("download");
    await save
      .getByRole("link", { name: `Download ${openingName}`, exact: true })
      .click();
    const downloaded = await download;
    assert.equal(downloaded.suggestedFilename(), openingName);
    assert.deepEqual(await readFile(await downloaded.path()), openingBytes);
    await save.getByRole("button", { name: "Close", exact: true }).click();
    await save.waitFor({ state: "hidden" });

    await dropFiles(editor, [
      replacement,
      ["Another bank.SC2Bank", openingBytes],
    ]);
    await status("Open one bank file at a time.").waitFor();
    assert.equal(await openCount(), 1);
    await heading(openingName).waitFor();

    await value.fill("002");
    await page
      .getByText("Unapplied Table View changes", { exact: true })
      .waitFor();
    await dropFiles(editor, [replacement]);
    assert.equal(await openCount(), 1);
    assert.equal(await value.inputValue(), "002");
    await page
      .getByRole("button", { name: "Apply edits", exact: true })
      .click();
    await status("Unsaved changes").waitFor();

    const guard = page.getByRole("dialog", {
      name: "Unsaved bank changes",
      exact: true,
    });
    async function expectGuard(files) {
      await dropFiles(editor, files);
      await guard.waitFor();
    }
    async function checkPreserved() {
      await heading(openingName).waitFor();
      await status("Unsaved changes").waitFor();
      assert.equal(await value.inputValue(), "002");
    }

    await expectGuard([replacement]);
    assert.equal(await openCount(), 1);
    await guard.getByRole("button", { name: "Cancel", exact: true }).click();
    await status("The current bank is still open.").waitFor();
    await checkPreserved();
    assert.equal(await openCount(), 1);

    await expectGuard([["Malformed drop.SC2Bank", Buffer.from("<Bank>")]]);
    await guard
      .getByRole("button", { name: "Discard and open", exact: true })
      .click();
    await status("The bank XML could not be opened safely.").waitFor();
    await checkPreserved();
    assert.equal(await openCount(), 2);

    await expectGuard([replacement]);
    await guard
      .getByRole("button", { name: "Discard and open", exact: true })
      .click();
    await heading(replacementName).waitFor();
    assert.equal(await value.inputValue(), "003");
    assert.equal(await openCount(), 3);
  } finally {
    await page.close();
  }
}
