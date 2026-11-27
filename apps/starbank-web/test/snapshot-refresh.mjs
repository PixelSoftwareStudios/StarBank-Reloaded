import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stagedValue = "9,007,199,254,740,993";
const acceptedValue = "9007199254740993";

function installProjectionFault(context, stopWorker) {
  return context.addInitScript((stopWorker) => {
    const BrowserWorker = globalThis.Worker;
    const fixture = { commands: [], failed: undefined };
    globalThis.fixtureSnapshotRefresh = fixture;
    globalThis.Worker = class extends BrowserWorker {
      #armed = !stopWorker;
      #awaitingProjection = false;
      #failedRequestId;

      constructor(workerUrl, options) {
        super(workerUrl, options);
        globalThis.addEventListener(
          "starbank-test-fail-snapshot",
          () => {
            this.#armed = true;
          },
          { once: true },
        );
        this.addEventListener("message", (event) => {
          if (
            this.#failedRequestId === undefined ||
            this.#failedRequestId !== event.data.id
          ) {
            return;
          }
          event.stopImmediatePropagation();
          this.#failedRequestId = undefined;
          const reply = JSON.parse(event.data.control);
          fixture.failed = {
            document_id: reply.document_id,
            revision: reply.revision,
          };
          this.dispatchEvent(
            new MessageEvent("message", {
              data: {
                ...event.data,
                control: JSON.stringify({
                  ...reply,
                  result: { kind: "raw_text", payload: {} },
                }),
              },
            }),
          );
        });
      }

      postMessage(request, transfer) {
        const { command, document_id, expected_revision } = JSON.parse(
          request.control,
        );
        fixture.commands.push({
          kind: command.kind,
          document_id,
          expected_revision,
        });
        if (
          this.#armed &&
          command.kind === "set_values" &&
          fixture.failed === undefined
        )
          this.#awaitingProjection = true;
        if (command.kind === "raw_text" && this.#awaitingProjection) {
          this.#awaitingProjection = false;
          if (stopWorker) {
            fixture.failed = { document_id, revision: expected_revision };
            this.dispatchEvent(new Event("error"));
            return;
          }
          this.#failedRequestId = request.id;
        }
        super.postMessage(request, transfer);
      }
    };
  }, stopWorker);
}

export async function verifySnapshotRefresh(
  browser,
  url,
  bankPicker,
  viewport,
) {
  const context = await browser.newContext({ viewport });
  try {
    await installProjectionFault(context, false);

    const page = await context.newPage();
    const errors = [];
    const requests = [];
    page.setDefaultTimeout(10_000);
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) => requests.push(request.url()));
    await page.goto(url);
    await bankPicker(page)(
      "SnapshotRefresh.SC2Bank",
      '<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section></Bank>',
    );
    const gold = page.getByRole("textbox", { name: "Gold · Value value" });
    const apply = page.getByRole("button", {
      name: "Apply edits",
      exact: true,
    });
    const save = page.getByRole("button", { name: "Save As…", exact: true });
    const discard = page.getByRole("button", {
      name: "Discard draft",
      exact: true,
    });
    const refresh = page.getByRole("button", {
      name: "Refresh editor",
      exact: true,
    });

    const exportDialog = page.getByRole("dialog", { name: "Save a bank copy" });
    await gold.fill(stagedValue);
    await gold.press("Control+s");
    await refresh.waitFor();
    await page
      .getByRole("status")
      .filter({
        hasText: "Refresh the editor to check the current bank revision.",
      })
      .waitFor();
    assert.equal(await gold.inputValue(), stagedValue);
    assert.equal(await apply.isDisabled(), true);
    assert.equal(await save.isDisabled(), true);
    assert.equal(await exportDialog.count(), 0);
    const before = await page.evaluate(() => globalThis.fixtureSnapshotRefresh);
    assert(before.failed);
    const edits = before.commands.filter(({ kind }) => kind === "set_values");
    assert.equal(edits.length, 1);
    assert.equal(edits[0].document_id, before.failed.document_id);
    assert.notEqual(edits[0].expected_revision, before.failed.revision);
    assert.equal(
      before.commands.some(({ kind }) => kind === "prepare"),
      false,
    );

    await refresh.click();
    await refresh.waitFor({ state: "hidden" });
    await page.getByRole("cell", { name: stagedValue, exact: true }).waitFor();
    await page.getByText("Review Table draft", { exact: true }).click();
    const original = page.getByRole("textbox", {
      name: "Original Table draft",
      exact: true,
    });
    const draft = await original.inputValue();
    assert.equal(await original.evaluate((input) => input.readOnly), true);
    assert.deepEqual(JSON.parse(draft), [
      { section: "Player", field: "Gold · Value", value: stagedValue },
    ]);
    await original.press("ControlOrMeta+a");
    assert.equal(
      await original.evaluate((input) =>
        input.value.slice(input.selectionStart, input.selectionEnd),
      ),
      draft,
    );
    assert.equal(await apply.isDisabled(), true);
    assert.equal(await save.isDisabled(), true);
    assert.equal(await exportDialog.count(), 0);
    const after = await page.evaluate(() => globalThis.fixtureSnapshotRefresh);
    assert.deepEqual(
      after.commands.slice(before.commands.length),
      ["snapshot", "raw_text"].map((kind) => ({
        kind,
        document_id: before.failed.document_id,
        expected_revision: before.failed.revision,
      })),
    );

    await discard.click();
    await page.getByRole("tab", { name: "Raw View", exact: true }).click();
    await page
      .getByRole("textbox", { name: "Bank XML" })
      .filter({ hasText: `int="${acceptedValue}"` })
      .waitFor();
    assert.equal(await save.isEnabled(), true);
    assert.deepEqual(errors, []);
    assert(
      requests.every(
        (request) => new URL(request).origin === new URL(url).origin,
      ),
    );
  } finally {
    await context.close();
  }
  await verifyTerminalSnapshotRecovery(browser, url, bankPicker, viewport);
}

async function verifyTerminalSnapshotRecovery(
  browser,
  url,
  bankPicker,
  viewport,
) {
  const context = await browser.newContext({ viewport, acceptDownloads: true });
  try {
    await installProjectionFault(context, true);
    const page = await context.newPage();
    page.setDefaultTimeout(10_000);
    await page.goto(url);
    const filename = "RecoverySnapshot.SC2Bank";
    const openingXml =
      '\uFEFF<?xml version="1.0" encoding="UTF-16"?>\r\n' +
      '<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section></Bank>\r\n';
    const lastDisplayedBytes = Buffer.from(
      openingXml.replace('int="001"', 'int="004"'),
      "utf16le",
    );
    await bankPicker(page)(filename, Buffer.from(openingXml, "utf16le"));
    const gold = page.getByRole("textbox", { name: "Gold · Value value" });
    const apply = page.getByRole("button", {
      name: "Apply edits",
      exact: true,
    });
    await gold.fill("004");
    await apply.click();
    await apply.waitFor({ state: "hidden" });
    assert.equal(await gold.inputValue(), "004");
    await page.evaluate(() =>
      globalThis.dispatchEvent(new Event("starbank-test-fail-snapshot")),
    );

    const recovery = page.getByRole("button", {
      name: "Download recovery copy…",
      exact: true,
    });
    const save = page.getByRole("button", { name: "Save As…", exact: true });
    await gold.fill(stagedValue);
    await gold.press("Control+s");
    await recovery.waitFor();
    assert.equal(
      await page
        .getByRole("button", { name: "Refresh editor", exact: true })
        .count(),
      0,
    );
    assert.equal(await save.isDisabled(), true);
    const before = await page.evaluate(() => globalThis.fixtureSnapshotRefresh);
    const edits = before.commands.filter(({ kind }) => kind === "set_values");
    assert.equal(edits.length, 2);
    assert(before.failed);
    assert.equal(edits[1].document_id, before.failed.document_id);
    assert.notEqual(edits[0].expected_revision, edits[1].expected_revision);
    assert.notEqual(edits[1].expected_revision, before.failed.revision);
    assert.equal(await gold.inputValue(), stagedValue);

    await recovery.click();
    const dialog = page.getByRole("dialog", { name: "Save a bank copy" });
    await dialog.waitFor();
    assert.equal(await dialog.getByLabel("Re-sign this copy").count(), 0);
    await dialog.getByRole("button", { name: "Prepare download" }).click();
    const warning = dialog
      .getByRole("status")
      .filter({ hasText: /last accepted revision/i });
    await warning.waitFor();
    assert.match(
      await warning.textContent(),
      /not include.*draft|excludes.*draft/i,
    );
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      dialog.getByRole("link", { name: `Download ${filename}` }).click(),
    ]);
    assert.equal(download.suggestedFilename(), filename);
    assert.deepEqual(await readFile(await download.path()), lastDisplayedBytes);
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    assert.equal(await gold.inputValue(), stagedValue);
    assert.equal(await save.isDisabled(), true);
    const after = await page.evaluate(() => globalThis.fixtureSnapshotRefresh);
    assert.deepEqual(after, before);
  } finally {
    await context.close();
  }
}
