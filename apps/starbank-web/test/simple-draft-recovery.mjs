import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

function installOutOfBandEdit(context, updatedXml) {
  return context.addInitScript((replacement) => {
    const BrowserWorker = globalThis.Worker;
    const trace = [];
    globalThis.fixtureSimpleRecoveryTrace = trace;

    globalThis.Worker = class extends BrowserWorker {
      #forwarding = false;
      #advanced = false;
      #commands = new Map();

      constructor(url, options) {
        super(url, options);
        this.addEventListener("message", (event) => {
          if (this.#forwarding || event.data.kind !== "response") return;
          event.stopImmediatePropagation();

          const reply = JSON.parse(event.data.control);
          trace.push({
            command: this.#commands.get(event.data.id),
            revision: reply.revision,
            result: reply.result.kind,
            status: reply.result.payload?.status,
            error: reply.result.payload?.code,
          });
          if (event.data.id % 2 === 1) return;

          this.#forwarding = true;
          this.dispatchEvent(
            new MessageEvent("message", {
              data: { ...event.data, id: event.data.id / 2 },
            }),
          );
          this.#forwarding = false;
        });
      }

      postMessage(request, transfer) {
        const control = JSON.parse(request.control);
        // Reserve the odd transport ID for one same-session revision advance.
        const id = request.id * 2;
        if (!this.#advanced && control.command.kind === "profile_apply") {
          this.#advanced = true;
          const bytes = new TextEncoder().encode(replacement).buffer;
          this.#commands.set(id - 1, "fixture_replace_raw_text");
          super.postMessage(
            {
              id: id - 1,
              bytes,
              control: JSON.stringify({
                ...control,
                request_id: "fixture-profile-refresh",
                command: { kind: "replace_raw_text" },
              }),
            },
            [bytes],
          );
        }
        this.#commands.set(id, control.command.kind);
        super.postMessage({ ...request, id }, transfer);
      }
    };
  }, updatedXml);
}

export async function verifySimpleDraftRecovery(
  browser,
  url,
  bankPicker,
  viewport,
) {
  const fixtures = new URL(
    "../../../crates/domain/starbank-profiles/tests/fixtures/",
    import.meta.url,
  );
  const [bank, profile] = await Promise.all([
    readFile(new URL("training.sc2bank", fixtures), "utf8"),
    readFile(new URL("training.json", fixtures)),
  ]);
  const context = await browser.newContext({ viewport });
  try {
    await installOutOfBandEdit(
      context,
      bank.replace('string="fixture-v1"', 'string="fixture-v2"'),
    );
    const page = await context.newPage();
    const errors = [];
    page.setDefaultTimeout(10_000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(url);
    await bankPicker(page)("TrainingBank.SC2Bank", bank);
    await page.getByRole("heading", { name: "TrainingBank.SC2Bank" }).waitFor();
    await page.getByText("Bank profile", { exact: true }).click();
    await page
      .getByLabel("Map author handle supplied by you")
      .fill("9-S2-1-123");
    await page.getByLabel("Bank profile file", { exact: true }).setInputFiles({
      name: "training.json",
      mimeType: "application/json",
      buffer: profile,
    });

    const simple = page.getByRole("tab", { name: "Simple View", exact: true });
    await simple.click();
    const panel = page.getByRole("tabpanel", {
      name: "Simple View",
      exact: true,
    });
    await panel.getByRole("textbox", { name: /^Score/ }).fill("2.00");
    await page
      .getByRole("button", { name: "Preview edits", exact: true })
      .click();
    await panel
      .getByRole("region", { name: "Proposed profile changes" })
      .getByRole("cell", { name: "12.25", exact: true })
      .waitFor();

    await page
      .getByRole("button", { name: "Apply preview", exact: true })
      .click();
    const refresh = page.getByRole("button", {
      name: "Refresh editor",
      exact: true,
    });
    await refresh.waitFor();
    const before = await page.evaluate(
      () => globalThis.fixtureSimpleRecoveryTrace,
    );
    assert.deepEqual(
      before
        .slice(-2)
        .map(({ command, result, error }) => ({ command, result, error })),
      [
        {
          command: "fixture_replace_raw_text",
          result: "snapshot",
          error: undefined,
        },
        { command: "profile_apply", result: "error", error: "stale_revision" },
      ],
    );
    const revision = before.at(-2).revision;

    await refresh.click();
    await refresh.waitFor({ state: "hidden" });
    const table = page.getByRole("tab", { name: "Table View", exact: true });
    assert.equal(await table.getAttribute("aria-selected"), "true");
    assert.equal(await simple.count(), 0);
    assert.equal(
      await page
        .getByRole("button", { name: "Apply preview", exact: true })
        .count(),
      0,
    );

    await page.getByText("Review Simple draft", { exact: true }).click();
    const original = page.getByRole("textbox", {
      name: "Original Simple draft",
      exact: true,
    });
    const text = await original.inputValue();
    assert.deepEqual(JSON.parse(text), {
      field: "Score",
      input: { kind: "text", value: "2.00" },
      changes: [
        { field: "Reward", previous: "6.25", value: "12.25" },
        { field: "Score", previous: "01.00", value: "2.00" },
        { field: "Total", previous: "2.25", value: "4.25" },
      ],
    });
    assert.equal(await original.evaluate((input) => input.readOnly), true);
    await original.press("ControlOrMeta+a");
    assert.equal(
      await original.evaluate((input) =>
        input.value.slice(input.selectionStart, input.selectionEnd),
      ),
      text,
    );

    const save = page.getByRole("button", { name: "Save As…", exact: true });
    assert.equal(await save.isDisabled(), true);
    const after = await page.evaluate(
      () => globalThis.fixtureSimpleRecoveryTrace,
    );
    assert.deepEqual(
      after.slice(before.length).map(({ command, revision, status }) => ({
        command,
        revision,
        status,
      })),
      [
        { command: "snapshot", revision, status: undefined },
        { command: "raw_text", revision, status: undefined },
        { command: "profile_snapshot", revision, status: "incompatible" },
      ],
    );

    await page
      .getByRole("button", { name: "Discard draft", exact: true })
      .click();
    await original.waitFor({ state: "detached" });
    assert.equal(await save.isEnabled(), true);
    await page.getByRole("tab", { name: "Raw View", exact: true }).click();
    const xml = page.getByRole("textbox", { name: "Bank XML" });
    await xml.filter({ hasText: 'string="fixture-v2"' }).waitFor();
    assert.match(await xml.textContent(), /<Score fixed='01.00'/u);
    assert.deepEqual(errors, []);
  } finally {
    await context.close();
  }
}
