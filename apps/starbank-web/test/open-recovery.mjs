import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const firstName = "First.SC2Bank";
const nextName = "Replacement.SC2Bank";
const firstXml =
  '<Bank><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section></Bank>';
const nextXml = firstXml.replace('"001"', '"007"');

async function installOpenFaults(context) {
  await context.addInitScript(() => {
    const BrowserWorker = globalThis.Worker;
    const fixture = { commands: [], acknowledged: [] };
    globalThis.fixtureOpenRecovery = fixture;
    globalThis.Worker = class extends BrowserWorker {
      #fault;
      #faultId;
      #openId;

      constructor(url, options) {
        super(url, options);
        globalThis.addEventListener(
          "starbank-test-open-fault",
          ({ detail }) => {
            this.#fault = detail;
          },
        );
        this.addEventListener("message", (event) => {
          if (this.#openId !== undefined && event.data.id === this.#openId) {
            this.#openId = undefined;
            const { document_id, revision } = JSON.parse(event.data.control);
            fixture.acknowledged.push({
              document_id,
              expected_revision: revision,
            });
          }
          if (this.#faultId === undefined || event.data.id !== this.#faultId)
            return;
          this.#faultId = undefined;
          event.stopImmediatePropagation();
          const reply = JSON.parse(event.data.control);
          this.dispatchEvent(
            new MessageEvent("message", {
              data: {
                ...event.data,
                control: JSON.stringify({
                  ...reply,
                  document_id: "unrelated-fixture",
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
        if (command.kind === "open") this.#openId = request.id;
        if (command.kind === "raw_text" && this.#fault) {
          const fault = this.#fault;
          this.#fault = undefined;
          if (fault === "stopped") {
            this.dispatchEvent(new Event("error"));
            return;
          }
          this.#faultId = request.id;
        }
        super.postMessage(request, transfer);
      }
    };
  });
}

function button(page, name) {
  return page.getByRole("button", { name, exact: true });
}

function status(page, message) {
  return page.getByRole("status").filter({ hasText: message });
}

function sent(page) {
  return page.evaluate(() => globalThis.fixtureOpenRecovery);
}

function armFault(page, detail) {
  return page.evaluate(
    (detail) =>
      globalThis.dispatchEvent(
        new CustomEvent("starbank-test-open-fault", { detail }),
      ),
    detail,
  );
}

async function assertRecoveryReads(page, before) {
  const next = await sent(page);
  const target = before.acknowledged.at(-1);
  assert(target);
  assert.deepEqual(next.acknowledged, before.acknowledged);
  assert.deepEqual(next.commands.slice(before.commands.length), [
    { kind: "snapshot", ...target },
    { kind: "raw_text", ...target },
  ]);
}

export async function verifyAcceptedOpenRecovery(
  browser,
  url,
  bankPicker,
  viewport,
) {
  const context = await browser.newContext({ viewport, acceptDownloads: true });
  try {
    await installOpenFaults(context);
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.setDefaultTimeout(10_000);
    await page.goto(url);

    const pick = bankPicker(page);
    const finish = button(page, "Finish opening bank");
    const open = button(page, "Open Bank…");
    const save = button(page, "Save As…");
    const gold = page.getByRole("textbox", { name: "Gold · Value value" });
    const projectionError = status(page, "its XML could not be displayed");
    const firstHeading = page.getByRole("heading", {
      name: firstName,
      exact: true,
    });
    const nextHeading = page.getByRole("heading", {
      name: nextName,
      exact: true,
    });

    await armFault(page, "unrelated-document");
    await pick(firstName, firstXml);
    await finish.waitFor();
    await projectionError.waitFor();
    assert(await open.isDisabled());
    assert.equal(await firstHeading.count(), 0);
    assert.equal(await gold.count(), 0);

    const firstAttempt = await sent(page);
    assert.equal(firstAttempt.acknowledged.length, 1);
    await armFault(page, "unrelated-document");
    await finish.click();
    await projectionError.waitFor();
    assert.equal(await firstHeading.count(), 0);
    await assertRecoveryReads(page, firstAttempt);

    const firstRetry = await sent(page);
    await finish.click();
    await firstHeading.waitFor();
    assert.equal(await finish.count(), 0);
    assert.equal(await gold.inputValue(), "001");
    await assertRecoveryReads(page, firstRetry);

    await gold.fill("002");
    await button(page, "Apply edits").click();
    await status(page, "Unsaved changes").waitFor();
    await armFault(page, "unrelated-document");
    await pick(nextName, nextXml);
    await button(page, "Discard and open").click();
    await projectionError.waitFor();
    await finish.waitFor();
    assert(await firstHeading.isVisible());
    assert.equal(await nextHeading.count(), 0);
    await page.getByRole("cell", { name: "002", exact: true }).waitFor();
    assert.equal(await gold.count(), 0);
    assert(await save.isDisabled());
    assert(await open.isDisabled());

    const replacement = await sent(page);
    assert.equal(replacement.acknowledged.length, 2);
    assert.notEqual(
      replacement.acknowledged[0].document_id,
      replacement.acknowledged[1].document_id,
    );
    await finish.click();
    await nextHeading.waitFor();
    await assertRecoveryReads(page, replacement);
    assert.equal(await gold.inputValue(), "007");
    assert(await save.isEnabled());

    await armFault(page, "stopped");
    await pick("Unavailable.SC2Bank", firstXml);
    await status(
      page,
      "The engine stopped before Unavailable.SC2Bank",
    ).waitFor();
    assert.equal(await finish.count(), 0);
    assert(await nextHeading.isVisible());
    assert(await open.isDisabled());
    assert(await save.isDisabled());
    const stopped = await sent(page);
    assert.equal(stopped.acknowledged.length, 3);

    await button(page, "Download recovery copy…").click();
    const dialog = page.getByRole("dialog", { name: "Save a bank copy" });
    assert.equal(await dialog.getByLabel("Re-sign this copy").count(), 0);
    await button(dialog, "Prepare download").click();
    await status(
      dialog,
      "It excludes drafts and revisions the editor could not display",
    ).waitFor();
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      dialog.getByRole("link", { name: `Download ${nextName}` }).click(),
    ]);
    assert.deepEqual(
      await readFile(await download.path()),
      Buffer.from(nextXml),
    );
    assert.deepEqual(await sent(page), stopped);
    assert.deepEqual(errors, []);
  } finally {
    await context.close();
  }
}
