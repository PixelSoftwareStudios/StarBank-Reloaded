import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, truncate, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export async function simulateUnavailableBank(context) {
  await context.addInitScript(() => {
    const fileBytes = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function () {
      if (this.name === "Unavailable.SC2Bank") {
        return Promise.reject(
          new DOMException("No longer readable", "NotReadableError"),
        );
      }
      return fileBytes.call(this);
    };
  });
}

export async function verifyLocalImports(page, initial) {
  await page.getByText("Local banks", { exact: true }).click();
  const library = page.locator("details.local-bank-library");
  const list = library.getByRole("list", { name: "Local bank list" });
  const files = [
    {
      name: "Alpha.SC2Bank",
      mimeType: "application/octet-stream",
      buffer: Buffer.from(initial),
    },
    {
      name: "Beta.XML",
      mimeType: "application/xml",
      buffer: Buffer.from(initial.replace('int="001"', 'int="002"')),
    },
    {
      name: "Unavailable.SC2Bank",
      mimeType: "application/octet-stream",
      buffer: Buffer.from(initial),
    },
    {
      name: "notes.txt",
      mimeType: "text/plain",
      buffer: Buffer.from("Ignore this file."),
    },
  ];
  const choosingFiles = page.waitForEvent("filechooser");
  await library.getByRole("button", { name: "Select bank files…" }).click();
  await (await choosingFiles).setFiles(files);
  await list.waitFor();
  assert.equal(await list.getByRole("listitem").count(), 3);
  assert.equal(
    await page.getByRole("heading", { name: "Alpha.SC2Bank" }).count(),
    0,
  );

  const alpha = list.getByRole("button", {
    name: "Alpha.SC2Bank",
    exact: true,
  });
  const beta = list.getByRole("button", { name: "Beta.XML", exact: true });
  await alpha.click();
  await page.getByRole("heading", { name: "Alpha.SC2Bank" }).waitFor();
  assert.equal(await alpha.getAttribute("aria-current"), "true");
  const value = page.getByRole("textbox", { name: "Gold · Value value" });
  await value.fill("009");
  await library
    .getByText("Apply or discard your draft before opening another bank.")
    .waitFor();
  assert.equal(await beta.isDisabled(), true);
  await page.getByRole("button", { name: "Apply edits" }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "Unsaved changes" })
    .waitFor();

  page.once("dialog", (confirmation) => confirmation.dismiss());
  await beta.click();
  await library
    .getByRole("status")
    .filter({ hasText: "The current bank is still open." })
    .waitFor();
  await page.getByRole("heading", { name: "Alpha.SC2Bank" }).waitFor();
  page.once("dialog", (confirmation) => confirmation.accept());
  await beta.click();
  await page.getByRole("heading", { name: "Beta.XML" }).waitFor();
  assert.equal(await value.inputValue(), "002");

  await list
    .getByRole("button", { name: "Unavailable.SC2Bank", exact: true })
    .click();
  await library
    .getByRole("status")
    .filter({ hasText: "Select it again and retry." })
    .waitFor();
  assert.equal(await beta.getAttribute("aria-current"), "true");

  const folder = await mkdtemp(join(tmpdir(), "starbank-browser-folder-"));
  try {
    const nested = join(folder, "nested");
    await mkdir(nested);
    const oversized = join(folder, "Oversized.SC2Bank");
    await writeFile(oversized, "");
    await truncate(oversized, 9 * 1024 * 1024);
    await writeFile(join(folder, "unrelated.xml"), initial);
    await writeFile(join(nested, "Folder.SC2BANK"), initial);
    const choosingFolder = page.waitForEvent("filechooser");
    await library.getByRole("button", { name: "Select bank folder…" }).click();
    await (await choosingFolder).setFiles(folder);
    const listedFile = list.getByRole("button", {
      name: /nested\/Folder\.SC2BANK/,
    });
    await listedFile.waitFor();
    assert.equal(await list.getByRole("listitem").count(), 1);
    assert.match(
      await library.getByRole("status").textContent(),
      /1 oversized bank/,
    );
    await page.getByRole("heading", { name: "Beta.XML" }).waitFor();
    await listedFile.click();
    await page.getByRole("heading", { name: "Folder.SC2BANK" }).waitFor();
  } finally {
    await rm(folder, { recursive: true, force: true });
  }
  await library.getByRole("button", { name: "Clear list" }).click();
  await list.waitFor({ state: "detached" });
  await page.getByRole("heading", { name: "Folder.SC2BANK" }).waitFor();
  await library.getByText("Local banks", { exact: true }).click();
}
