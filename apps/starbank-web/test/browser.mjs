import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { verifyDiffContext } from "./diff-context.mjs";
import { verifyTableDraftSwitch } from "./draft-switch.mjs";
import { verifyDropImport } from "./drop-import.mjs";
import {
  simulateUnavailableBank,
  verifyLocalImports,
} from "./local-bank-library.mjs";
import { verifyNumericCaret } from "./numeric-caret.mjs";
import { verifyAcceptedOpenRecovery } from "./open-recovery.mjs";
import { verifyRawSyntaxDiagnostics } from "./raw-syntax-diagnostics.mjs";
import { verifySaveMenu } from "./save-menu.mjs";
import { verifySimpleDraftRecovery } from "./simple-draft-recovery.mjs";
import { verifySnapshotRefresh } from "./snapshot-refresh.mjs";
import { verifyStructureDraftClose } from "./structure-draft-close.mjs";
import { verifyWindowedTable } from "./windowed-table.mjs";

const playwright = await import(
  process.env.STARBANK_TEST_PLAYWRIGHT_MODULE ?? "playwright"
);
const supported = ["chromium", "firefox", "webkit"];
const engines = process.argv.slice(2);
assert(
  engines.every((engine) => supported.includes(engine)),
  "Unknown browser requested",
);

const dist = fileURLToPath(new URL("../dist/", import.meta.url));
const documentName = "Synthetic.SC2Bank";
const initial =
  '<?xml version="1.0"?>\r\n<Bank version="1"><Section name="Player"><Key name="Gold"><Value int="001"/></Key></Section><!-- synthetic --></Bank>\r\n';
const edited = initial.replace('int="001"', 'int="1000000"');
const rawEdited = edited
  .replaceAll("\r\n", "")
  .replace("synthetic", "raw draft applied");
const contentTypes = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".wasm": "application/wasm",
};
const mountPath = "/starbank/";

async function startServer() {
  const [rule, ...headerLines] = (
    await readFile(join(dist, "_headers"), "utf8")
  )
    .trimEnd()
    .split(/\r?\n/);
  assert.equal(rule, "/*");
  const staticHeaders = Object.fromEntries(
    headerLines.map((line) => {
      const header = /^\s+([^:]+): (.+)$/.exec(line);
      assert(header, `Malformed static header ${line}`);
      return [header[1], header[2]];
    }),
  );
  assert(staticHeaders["Content-Security-Policy"]);

  const requests = [];
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    requests.push([request.method, url.pathname]);
    const asset = url.pathname.slice(mountPath.length) || "index.html";
    const file = resolve(join(dist, asset));
    if (
      request.method !== "GET" ||
      !url.pathname.startsWith(mountPath) ||
      !file.startsWith(resolve(dist) + sep) ||
      asset === "_headers"
    ) {
      response.writeHead(404).end();
      return;
    }
    try {
      const data = await readFile(file);
      response
        .writeHead(200, {
          ...staticHeaders,
          "Cache-Control": "no-store",
          "Content-Type":
            contentTypes[extname(file)] ?? "application/octet-stream",
        })
        .end(data);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise((resolveListen, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveListen);
  });
  const origin = `http://127.0.0.1:${server.address().port}`;
  return {
    origin,
    url: `${origin}${mountPath}`,
    requests,
    stop: async () => {
      if (!server.listening) return;
      server.closeAllConnections();
      await new Promise((resolveClose) => server.close(resolveClose));
    },
  };
}

function bankPicker(page) {
  const fileInput = page.getByLabel("Bank file", { exact: true });
  return async (fileName, xml) => {
    await page.waitForFunction(() => {
      const input = document.querySelector('input[aria-label="Bank file"]');
      return input instanceof HTMLInputElement && !input.disabled;
    });
    await fileInput.setInputFiles({
      name: fileName,
      mimeType: "application/octet-stream",
      buffer: Buffer.from(xml),
    });
  };
}

async function downloadCopy(page, expected, shortcut) {
  if (shortcut) await page.keyboard.press(shortcut);
  else await page.getByRole("button", { name: "Save As…" }).click();
  const dialog = page.getByRole("dialog", { name: "Save a bank copy" });
  await dialog.getByRole("button", { name: "Prepare download" }).click();
  const download = page.waitForEvent("download");
  await dialog.getByRole("link", { name: `Download ${documentName}` }).click();
  const copy = await download;
  assert.equal(copy.suggestedFilename(), documentName);
  assert.deepEqual(await readFile(await copy.path()), Buffer.from(expected));
  return dialog;
}

async function downloadOpeningBackup(page, expected) {
  await page.getByRole("button", { name: "Save As…" }).click();
  const dialog = page.getByRole("dialog", { name: "Save a bank copy" });
  await dialog.getByLabel("Re-sign this copy").check();
  await dialog.getByRole("button", { name: "Prepare opening backup" }).click();
  await dialog
    .getByRole("status")
    .filter({ hasText: "exact file bytes" })
    .waitFor();
  const pending = page.waitForEvent("download");
  await dialog.getByRole("link", { name: /\.sc2bank\.bak$/ }).click();
  const backup = await pending;
  assert.match(backup.suggestedFilename(), /_\d{8}_\d{6}\.sc2bank\.bak$/);
  assert.deepEqual(await readFile(await backup.path()), Buffer.from(expected));
  await dialog.getByRole("button", { name: "Close", exact: true }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "Unsaved changes" })
    .waitFor();
}

async function verifyPreferences(page) {
  await page.getByRole("button", { name: "More…" }).click();
  const settings = page.getByRole("region", { name: "Settings" });
  const back = settings.getByRole("button", { name: "Back to editor" });
  await back.waitFor();
  await settings.getByLabel("Theme").selectOption("dark");
  await settings.getByLabel("Density").selectOption("comfortable");
  await settings.getByLabel("Text size").selectOption("large");
  await settings.getByLabel("Map list width").press("End");
  await settings.getByLabel("Table layout").selectOption("unified");
  await settings.getByLabel("Wrap Raw View lines").check();
  await settings.getByLabel("Hide Blizzard maps").uncheck();
  await settings.getByLabel("Auto re-sign banks on save").uncheck();
  await back.click();
  assert.equal(
    await page.getByLabel("Auto re-sign banks on save").isChecked(),
    false,
  );

  await page.reload();
  await page.getByRole("button", { name: "Open Bank…" }).waitFor();
  const shell = page.locator(".starbank-shell");
  assert.equal(await shell.getAttribute("data-density"), "comfortable");
  assert.equal(
    await page
      .locator("html")
      .evaluate((root) => getComputedStyle(root).fontSize),
    "18px",
  );
  assert.equal(
    await shell.evaluate(
      (element) => getComputedStyle(element).backgroundColor,
    ),
    "rgb(20, 21, 24)",
  );
  assert.equal(
    await page
      .getByRole("separator", { name: "Resize map list" })
      .getAttribute("aria-valuenow"),
    "50",
  );

  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await settings.getByLabel("Theme").waitFor();
  assert.equal(await settings.getByLabel("Theme").inputValue(), "dark");
  assert.equal(
    await settings.getByLabel("Hide Blizzard maps").isChecked(),
    false,
  );
  await back.click();
}

async function verifyStructuralEditing(page, value) {
  const changeStructure = async (action, configure) => {
    await page.getByRole("button", { name: "Edit structure…" }).click();
    const dialog = page.getByRole("dialog", { name: "Edit bank structure" });
    await dialog.getByLabel("Action").selectOption({ label: action });
    await configure(dialog);
    await dialog.getByRole("button", { name: action, exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
  };

  await value.fill("003");
  await page
    .getByText("Unapplied Table View changes", { exact: true })
    .waitFor();
  assert.equal(
    await page.evaluate(() =>
      window.dispatchEvent(new Event("beforeunload", { cancelable: true })),
    ),
    false,
    "A staged draft must guard page navigation",
  );
  await page.getByRole("button", { name: "Discard draft" }).click();
  assert.equal(
    await page.evaluate(() =>
      window.dispatchEvent(new Event("beforeunload", { cancelable: true })),
    ),
    true,
    "A discarded draft must release the navigation guard",
  );

  await changeStructure("Add section", (dialog) =>
    dialog.getByLabel("Name", { exact: true }).fill("Temporary"),
  );
  await changeStructure("Add key", async (dialog) => {
    await dialog.getByLabel(/^Section/).selectOption({ label: "Temporary" });
    await dialog.getByLabel("Name", { exact: true }).fill("EmptyKey");
  });
  await changeStructure("Add value", async (dialog) => {
    await dialog.getByLabel(/^Section/).selectOption({ label: "Temporary" });
    await dialog.getByLabel(/^Key/).selectOption({ label: "EmptyKey" });
    await dialog.getByLabel(/^Value type/).selectOption("int");
    await dialog.getByLabel("Value", { exact: true }).fill("007");
  });
  assert.equal(
    await page
      .getByRole("textbox", { name: "EmptyKey · Value value" })
      .inputValue(),
    "007",
  );

  await changeStructure("Rename key", async (dialog) => {
    await dialog.getByLabel(/^Section/).selectOption({ label: "Player" });
    await dialog.getByLabel(/^Key/).selectOption({ label: "Gold" });
    await dialog.getByLabel("Name", { exact: true }).fill("Coins");
  });
  await page.getByRole("button", { name: "Changes…" }).click();
  const diff = page.getByRole("dialog", { name: "Changes since opening" });
  await diff.getByText("Changed key Player › Coins", { exact: true }).waitFor();
  await diff.getByRole("button", { name: "Revert key Player › Coins" }).click();
  await diff.waitFor({ state: "hidden" });
  assert.equal(await value.inputValue(), "002");

  await page.getByRole("tab", { name: "Raw View" }).click();
  await page
    .locator(".cm-content")
    .filter({ hasText: /name="EmptyKey"/ })
    .waitFor();
  await page.getByRole("tab", { name: "Table View" }).click();
  page.once("dialog", (confirmation) => confirmation.accept());
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await page.waitForFunction(() =>
    window.dispatchEvent(new Event("beforeunload", { cancelable: true })),
  );
}

async function verifySimpleView(page, chooseBank) {
  const profileFixtures = new URL(
    "../../../crates/domain/starbank-profiles/tests/fixtures/",
    import.meta.url,
  );
  const profile = JSON.parse(
    await readFile(new URL("training.json", profileFixtures), "utf8"),
  );
  const trainingBank = await readFile(
    new URL("training.sc2bank", profileFixtures),
    "utf8",
  );
  const extraFields = [];
  for (let index = 0; index < 32; index += 1) {
    const name = `Fixture field ${index + 1}`;
    extraFields.push(`<Key name="${name}"><Value string="synthetic"/></Key>`);
    profile.groups[1].fields.push({
      id: `extra-${index}`,
      label: name,
      path: { section: { name: "Stats" }, key: { name } },
      control: { kind: "text", storage: "string" },
    });
  }
  const bank = trainingBank.replace(
    '<Key name="Note">',
    `${extraFields.join("")}<Key name="Note">`,
  );
  await chooseBank("TrainingBank.SC2Bank", bank);
  await page.getByRole("heading", { name: "TrainingBank.SC2Bank" }).waitFor();
  const tab = page.getByRole("tab", { name: "Simple View" });
  assert.equal(await tab.count(), 0);

  await page.getByText("Bank profile", { exact: true }).click();
  const authorInput = page.getByLabel("Map author handle supplied by you");
  const fileInput = page.getByLabel("Bank profile file", { exact: true });
  const loadProfile = async (author) => {
    await authorInput.fill(author);
    await fileInput.setInputFiles({
      name: "training.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(profile)),
    });
  };
  await loadProfile("9-S2-1-124");
  await page
    .getByRole("status")
    .filter({
      hasText: "The loaded profile belongs to a different bank identity.",
    })
    .waitFor();
  assert.equal(await tab.count(), 0);
  await loadProfile("9-S2-1-123");
  await tab.click();
  const panel = page.getByRole("tabpanel", { name: "Simple View" });
  const score = panel.getByRole("textbox", { name: /^Score/ });
  await score.waitFor();
  assert.equal(await score.inputValue(), "01.00");
  const rawTab = page.getByRole("tab", { name: "Raw View" });
  const tableTab = page.getByRole("tab", { name: "Table View" });
  const decision = page.getByRole("dialog", {
    name: "Simple draft needs review",
  });
  const preview = panel.getByRole("region", {
    name: "Proposed profile changes",
  });
  const xml = page.getByRole("textbox", { name: "Bank XML" });

  const simpleFields = panel.locator(".simple-fields");
  const scroll = await simpleFields.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
    return element.scrollTop;
  });
  assert(scroll > 0);
  await tableTab.click();
  await tab.click();
  assert.equal(
    await simpleFields.evaluate((element) => element.scrollTop),
    scroll,
  );
  await rawTab.click();
  await xml.waitFor();
  await tab.click();
  assert.equal(
    await simpleFields.evaluate((element) => element.scrollTop),
    scroll,
  );
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Back to editor" }).click();
  assert.equal(
    await simpleFields.evaluate((element) => element.scrollTop),
    scroll,
  );

  async function stagePreview() {
    await score.fill("2.00");
    await page.getByRole("button", { name: "Preview edits" }).click();
    await preview.getByRole("cell", { name: "12.25", exact: true }).waitFor();
    await preview.getByRole("cell", { name: "4.25", exact: true }).waitFor();
  }

  await score.fill("2.00");
  assert(
    await page
      .getByRole("button", { name: "Save As…", exact: true })
      .isDisabled(),
  );
  await rawTab.press("Enter");
  await decision.getByRole("button", { name: "Review edits" }).click();
  await page.waitForFunction(() =>
    document.activeElement?.matches(".simple-field input"),
  );
  assert.equal(await tab.getAttribute("aria-selected"), "true");
  assert.equal(await score.inputValue(), "2.00");
  assert.equal(await xml.isVisible(), false);

  await stagePreview();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await preview.waitFor({ state: "hidden" });
  await page.getByRole("button", { name: "Back to editor" }).click();
  await preview.getByRole("cell", { name: "12.25", exact: true }).waitFor();
  await preview.getByRole("cell", { name: "4.25", exact: true }).waitFor();

  await tableTab.click();
  await decision.waitFor();
  await page.keyboard.press("Escape");
  await decision.waitFor({ state: "detached" });
  assert.equal(await tab.getAttribute("aria-selected"), "true");
  await preview.getByRole("cell", { name: "12.25", exact: true }).waitFor();

  await rawTab.press("Space");
  await decision.getByRole("button", { name: "Discard and switch" }).click();
  await xml.filter({ hasText: /<Score fixed='01.00'/ }).waitFor();
  assert.equal(
    await page.getByRole("button", { name: "Apply preview" }).count(),
    0,
  );
  await tab.click();
  assert.equal(await score.inputValue(), "01.00");
  assert.equal(await preview.isVisible(), false);

  await stagePreview();
  await page.getByRole("button", { name: "Apply preview" }).click();
  await preview.waitFor({ state: "hidden" });

  await rawTab.click();
  await xml.filter({ hasText: /<Score fixed='2.00'/ }).waitFor();
  assert.match(await xml.textContent(), /<Reward fixed="12\.25"/u);
  assert.match(await xml.textContent(), /<Total fixed="4\.25"/u);
  await page.getByRole("button", { name: "Undo" }).click();
  await xml.filter({ hasText: /<Score fixed='01.00'/ }).waitFor();
  await tab.click();
  assert.equal(await score.inputValue(), "01.00");
  await page.getByRole("button", { name: "Clear profile" }).click();
  await tab.waitFor({ state: "detached" });
}

async function verifyRawViewPosition(page) {
  const comments = Array.from(
    { length: 200 },
    (_, index) => `<!-- Comment ${index} -->`,
  ).join("\n");
  await bankPicker(page)(
    "Raw view position.SC2Bank",
    `<Bank>\n${comments}\n</Bank>`,
  );
  const rawTab = page.getByRole("tab", { name: "Raw View" });
  const tableTab = page.getByRole("tab", { name: "Table View" });
  const raw = page.locator(".cm-content");
  const scroller = page.locator(".cm-scroller");
  assert.equal(
    await raw.count(),
    0,
    "Raw editor should mount only when opened",
  );
  await rawTab.click();
  await raw.waitFor();
  await scroller.evaluate((element) => {
    element.scrollTop = element.scrollHeight / 2;
  });
  await page.waitForFunction(
    () => document.querySelector(".cm-scroller")?.scrollTop > 100,
  );
  await raw.getByText("<!-- Comment 100 -->", { exact: true }).click();
  await raw.press("Shift+ArrowLeft");
  const selection = await page.evaluate(() =>
    window.getSelection()?.toString(),
  );
  assert.equal(selection?.length, 1, "Raw XML has a selected character");
  const scrollTop = await scroller.evaluate((element) => element.scrollTop);

  await tableTab.click();
  assert.equal(await tableTab.getAttribute("aria-selected"), "true");
  await raw.waitFor({ state: "hidden" });
  assert.equal(
    await page.getByRole("tabpanel", { name: "Raw View" }).count(),
    0,
    "The inactive Raw view must be hidden from accessibility navigation",
  );
  await rawTab.click();
  await raw.waitFor();
  const returnedScrollTop = await scroller.evaluate(
    (element) => element.scrollTop,
  );
  assert(
    Math.abs(returnedScrollTop - scrollTop) <= 8,
    `The Raw viewport moved from ${scrollTop} to ${returnedScrollTop}`,
  );
  await raw.focus();
  assert.equal(
    await page.evaluate(() => window.getSelection()?.toString()),
    selection,
    "The Raw selection should survive a clean round trip",
  );
}

async function verify(name) {
  const browser = await playwright[name].launch({ timeout: 15_000 });
  const server = await startServer();
  try {
    const context = await browser.newContext({ acceptDownloads: true });
    await simulateUnavailableBank(context);
    await context.addInitScript(() => {
      const BrowserWorker = globalThis.Worker;
      globalThis.Worker = class extends BrowserWorker {
        #holdNextValidation = false;
        #heldValidationId;

        constructor(url, options) {
          super(url, options);
          globalThis.addEventListener("starbank-test-hold-validation", () => {
            this.#holdNextValidation = true;
          });
          this.addEventListener("message", (event) => {
            if (
              this.#heldValidationId === undefined ||
              event.data.id !== this.#heldValidationId
            )
              return;
            event.stopImmediatePropagation();
            this.#heldValidationId = undefined;
            globalThis.releaseFixtureValidation = () => {
              delete globalThis.releaseFixtureValidation;
              this.dispatchEvent(
                new MessageEvent("message", { data: event.data }),
              );
            };
          });
          globalThis.addEventListener(
            "starbank-test-stop-worker",
            () => {
              this.dispatchEvent(new Event("error"));
            },
            { once: true },
          );
        }

        postMessage(request, transfer) {
          if (
            this.#holdNextValidation &&
            JSON.parse(request.control).command.kind === "validate_values"
          ) {
            this.#holdNextValidation = false;
            this.#heldValidationId = request.id;
          }
          super.postMessage(request, transfer);
        }
      };
      Object.defineProperty(navigator, "clipboard", {
        value: {
          write: () =>
            Promise.reject(
              new DOMException("Denied by the test", "NotAllowedError"),
            ),
        },
      });
    });
    const browserErrors = [];
    const requests = [];
    context.setDefaultTimeout(10_000);
    context.on("request", (request) => requests.push(request.url()));
    context.on("page", (page) =>
      page.on("pageerror", (error) => browserErrors.push(error.message)),
    );

    const viewport =
      name === "firefox"
        ? { width: 390, height: 640 }
        : { width: 850, height: 500 };
    await verifyDropImport(context, server.url, viewport);
    await verifySnapshotRefresh(browser, server.url, bankPicker, viewport);
    await verifyDiffContext(browser, server.url, bankPicker);
    await verifyNumericCaret(browser, server.url, bankPicker);
    await verifySimpleDraftRecovery(browser, server.url, bankPicker, viewport);
    await verifyAcceptedOpenRecovery(browser, server.url, bankPicker, viewport);
    const tableDraftPage = await context.newPage();
    try {
      await tableDraftPage.setViewportSize(viewport);
      await tableDraftPage.goto(server.url);
      await verifyTableDraftSwitch(tableDraftPage, bankPicker(tableDraftPage));
    } finally {
      await tableDraftPage.close();
    }

    const windowedPage = await context.newPage();
    try {
      await windowedPage.setViewportSize(viewport);
      await windowedPage.goto(server.url);
      const { initialRows, scrolledRows } = await verifyWindowedTable(
        windowedPage,
        bankPicker(windowedPage),
      );
      console.log(
        `${name} ${browser.version()} mounted ${initialRows} initial rows and ${scrolledRows} distant rows of 10,000 at ${viewport.width}×${viewport.height}. Focus, bulk edit, undo, and display filtering and sorting passed.`,
      );
    } finally {
      await windowedPage.close();
    }

    const rawPositionPage = await context.newPage();
    try {
      await rawPositionPage.setViewportSize({ width: 850, height: 500 });
      await rawPositionPage.goto(server.url);
      await verifyRawViewPosition(rawPositionPage);
      await verifyRawSyntaxDiagnostics(
        rawPositionPage,
        bankPicker(rawPositionPage),
      );
    } finally {
      await rawPositionPage.close();
    }

    const page = await context.newPage();
    await page.goto(server.url);
    await page.getByRole("button", { name: "Open Bank…" }).waitFor();
    await verifyPreferences(page);
    assert(
      server.requests.some(([, path]) => path.endsWith(".wasm")),
      `${name}: WASM was not fetched`,
    );
    await server.stop();
    const assetRequests = requests.length;

    const chooseBank = bankPicker(page);
    await verifyLocalImports(page, initial);
    await chooseBank(documentName, initial);
    const value = page.getByRole("textbox", { name: "Gold · Value value" });
    await value.waitFor();
    assert.equal(await value.inputValue(), "001");
    await page.getByRole("columnheader", { name: "Section" }).waitFor();
    const section = page.getByRole("cell", { name: "Player", exact: true });
    assert.equal(await section.getAttribute("aria-description"), null);

    await value.fill("1,000,000");
    assert.equal(await section.getAttribute("aria-description"), null);
    await downloadCopy(page, edited, "Control+s");
    await page.keyboard.press("Escape");
    await page.getByRole("tab", { name: "Table View" }).focus();
    await page
      .getByRole("status")
      .filter({ hasText: "Unsaved changes in 1 section" })
      .waitFor();
    assert.equal(
      await section.getAttribute("aria-description"),
      "Unsaved changes",
    );
    await section.locator(".editor-section-dirty").waitFor();
    assert.equal(await value.inputValue(), "1,000,000");
    await downloadOpeningBackup(page, initial);

    await page.getByRole("tab", { name: "Raw View" }).click();
    const raw = page.locator(".cm-content");
    await raw.waitFor();
    assert.match(await raw.getAttribute("class"), /cm-lineWrapping/);
    assert.match(await raw.textContent(), /int="1000000"/u);
    await raw.fill("<Bank>");
    await page.keyboard.press("Meta+s");
    await page
      .getByRole("status")
      .filter({ hasText: /bank XML could not be opened safely/i })
      .first()
      .waitFor();
    assert.equal(
      await page.getByRole("dialog", { name: "Save a bank copy" }).count(),
      0,
    );
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("button", { name: "Back to editor" }).click();
    assert.equal(await raw.textContent(), "<Bank>");
    await page.getByRole("button", { name: "Apply XML" }).click();
    await page
      .getByRole("status")
      .filter({
        hasText: /bank XML could not be opened safely/i,
      })
      .first()
      .waitFor();
    assert.equal(await raw.textContent(), "<Bank>");

    const tableTab = page.getByRole("tab", { name: "Table View" });
    const rawTab = page.getByRole("tab", { name: "Raw View" });
    const rawDecision = page.getByRole("dialog", {
      name: "Raw draft could not be applied",
    });
    await tableTab.click();
    await rawDecision
      .getByText("The bank XML could not be opened safely.")
      .waitFor();
    await page.keyboard.press("Escape");
    await rawDecision.waitFor({ state: "hidden" });
    assert.equal(await raw.textContent(), "<Bank>");
    assert.equal(await rawTab.getAttribute("aria-selected"), "true");

    await tableTab.press("Enter");
    await rawDecision.getByRole("button", { name: "Repair XML" }).click();
    await rawDecision.waitFor({ state: "hidden" });
    await page.waitForFunction(() =>
      document.activeElement?.matches(".cm-content"),
    );
    assert.equal(await raw.textContent(), "<Bank>");

    await tableTab.click();
    await rawDecision
      .getByRole("button", { name: "Discard and switch" })
      .click();
    await rawDecision.waitFor({ state: "hidden" });
    await value.waitFor();
    assert.equal(await value.inputValue(), "1,000,000");

    await rawTab.click();
    await raw.filter({ hasText: "<!-- synthetic -->" }).waitFor();
    await raw.press("ControlOrMeta+a");
    await page.keyboard.insertText(rawEdited);
    await tableTab.click();
    await value.waitFor();
    assert.equal(await tableTab.getAttribute("aria-selected"), "true");
    await rawTab.click();
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();
    await rawTab.focus();
    await page.keyboard.press("Meta+z");
    await raw.filter({ hasText: "<!-- synthetic -->" }).waitFor();
    await page.keyboard.press("Control+Shift+z");
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();

    const rawExported = rawEdited.replace(
      "raw draft applied",
      "raw draft exported",
    );
    await raw.press("ControlOrMeta+a");
    await page.keyboard.insertText(rawExported);
    const rawCopy = await downloadCopy(page, rawExported, "Meta+s");
    await rawCopy.getByRole("button", { name: "Close", exact: true }).click();
    await rawTab.focus();
    await page.keyboard.press("Meta+z");
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();

    let dialog = await downloadCopy(page, rawEdited, "Meta+s");
    await dialog.getByRole("button", { name: "Back" }).click();
    await dialog.getByLabel("Re-sign this copy").check();
    await dialog.getByLabel("Map author handle").fill("1-S2-1-999");
    await dialog.getByLabel("Player handle").fill("1-S2-1-321");
    await dialog.getByRole("button", { name: "Prepare download" }).click();
    await dialog.getByText("Signature verified.", { exact: false }).waitFor();
    const signedDownload = page.waitForEvent("download");
    await dialog
      .getByRole("link", { name: `Download ${documentName}` })
      .click();
    const signed = await signedDownload;
    const signedXml = await readFile(await signed.path(), "utf8");
    assert.match(signedXml, /<Signature value="[0-9A-F]{40}"\s*\/>/u);
    assert.equal(
      signedXml.replace(/<Signature value="[0-9A-F]{40}"\s*\/>/u, ""),
      rawEdited,
    );
    await page.keyboard.press("Escape");

    await raw.press("ControlOrMeta+a");
    await page.keyboard.insertText(rawExported);
    await verifySaveMenu(page);
    dialog = page.getByRole("dialog", { name: "Copy bank XML" });
    const clipboard = await dialog.getByLabel("Prepared bank XML").inputValue();
    assert.equal(
      clipboard.replaceAll("\r\n", "\n"),
      rawExported.replaceAll("\r\n", "\n"),
    );
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    await rawTab.focus();
    await page.keyboard.press("Meta+z");
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();

    const switchBank = page.getByRole("dialog", {
      name: "Unsaved bank changes",
    });
    await chooseBank("Replacement.SC2Bank", initial);
    await switchBank.getByRole("button", { name: "Cancel" }).click();
    await page
      .getByRole("status")
      .filter({ hasText: "The current bank is still open." })
      .waitFor();

    await chooseBank("Malformed.SC2Bank", "<Bank>");
    await switchBank.getByRole("button", { name: "Discard and open" }).click();
    await page
      .getByRole("status")
      .filter({ hasText: "The bank XML could not be opened safely." })
      .waitFor();
    await page.getByRole("heading", { name: documentName }).waitFor();
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();

    await chooseBank(
      "Replacement.SC2Bank",
      initial.replace('int="001"', 'int="002"'),
    );
    await switchBank.getByRole("button", { name: "Discard and open" }).click();
    await page.getByRole("heading", { name: "Replacement.SC2Bank" }).waitFor();
    assert.equal(await value.inputValue(), "002");
    await verifyStructureDraftClose(page, value);
    await verifyStructuralEditing(page, value);
    await verifySimpleView(page, chooseBank);
    const utf16Opening = initial
      .replace('version="1.0"?>', 'version="1.0" encoding="UTF-16"?>')
      .replace('int="001"', 'int="002"');
    const utf16 = (xml) => Buffer.from(`\uFEFF${xml}`, "utf16le");
    await chooseBank("Replacement.SC2Bank", utf16(utf16Opening));
    await page.getByRole("heading", { name: "Replacement.SC2Bank" }).waitFor();
    await value.fill("003");
    await page.getByRole("button", { name: "Apply edits" }).click();
    await page
      .getByRole("status")
      .filter({ hasText: "Unsaved changes" })
      .waitFor();
    await page.getByRole("tab", { name: "Raw View" }).click();
    await raw.filter({ hasText: /int="003"/ }).waitFor();
    const editedUtf16Xml = utf16Opening
      .replaceAll("\r\n", "")
      .replace('int="002"', 'int="004"');
    await raw.press("ControlOrMeta+a");
    await page.keyboard.insertText(editedUtf16Xml);
    await page.getByRole("button", { name: "Apply XML" }).click();
    await page
      .getByRole("button", { name: "Apply XML" })
      .waitFor({ state: "hidden" });
    await downloadOpeningBackup(page, utf16(utf16Opening));

    await page.evaluate(() =>
      globalThis.dispatchEvent(new Event("starbank-test-stop-worker")),
    );
    await page.getByRole("button", { name: "Save As…" }).click();
    dialog = page.getByRole("dialog", { name: "Save a bank copy" });
    await dialog.getByLabel("Re-sign this copy").check();
    await dialog.getByLabel("Map author handle").fill("1-S2-1-999");
    await dialog.getByLabel("Player handle").fill("1-S2-1-321");
    await dialog.getByRole("button", { name: "Prepare download" }).click();
    await dialog
      .getByRole("alert")
      .filter({ hasText: "Clear Re-sign this copy" })
      .waitFor();
    await dialog.getByLabel("Re-sign this copy").uncheck();
    await dialog.getByRole("button", { name: "Prepare download" }).click();
    await dialog
      .getByRole("status")
      .filter({
        hasText: "This recovery copy contains the last accepted revision.",
      })
      .waitFor();
    const recoveryLink = dialog.getByRole("link", {
      name: "Download Replacement.SC2Bank",
    });
    await page.waitForFunction(() =>
      document.activeElement?.matches("dialog a[download]"),
    );
    const pendingRecovery = page.waitForEvent("download");
    await recoveryLink.press("Enter");
    const recovery = await pendingRecovery;
    assert.deepEqual(
      await readFile(await recovery.path()),
      utf16(editedUtf16Xml),
    );
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });

    assert.deepEqual(browserErrors, []);
    const networkUrls = requests.filter((url) => url.startsWith("http"));
    assert(networkUrls.every((url) => new URL(url).origin === server.origin));
    assert(
      requests.slice(assetRequests).every((url) => url.startsWith("blob:")),
    );
    console.log(
      `${name} ${browser.version()} passed offline single, multiple and folder import, table/XML edits, undo/redo, exact, signed, and opening backup downloads, clipboard fallback, structural editing and reverts, Simple profile previews, guarded file switching, keyboard-accessible recovery, and persisted settings`,
    );
  } finally {
    await server.stop();
    await browser.close();
  }
}

for (const engine of engines.length ? engines : supported) {
  await verify(engine);
}
