import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import {
  simulateUnavailableBank,
  verifyLocalImports,
} from "./local-bank-library.mjs";
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
const csp = [
  "default-src 'none'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self' 'unsafe-inline'",
  "connect-src 'self'",
  "worker-src 'self'",
  "img-src 'self' data:",
  "base-uri 'none'",
  "form-action 'none'",
].join("; ");

async function startServer() {
  const requests = [];
  const server = createServer(async (request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    requests.push([request.method, url.pathname]);
    const file = resolve(
      join(dist, url.pathname === "/" ? "index.html" : url.pathname),
    );
    if (request.method !== "GET" || !file.startsWith(resolve(dist) + sep)) {
      response.writeHead(404).end();
      return;
    }
    try {
      const data = await readFile(file);
      response
        .writeHead(200, {
          "Cache-Control": "no-store",
          "Content-Security-Policy": csp,
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
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
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
  await diff.getByText("Changed key Coins", { exact: true }).waitFor();
  await diff.getByRole("button", { name: "Revert key Coins" }).click();
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
  const profile = await readFile(new URL("training.json", profileFixtures));
  const bank = await readFile(
    new URL("training.sc2bank", profileFixtures),
    "utf8",
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
      buffer: profile,
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
  await score.fill("2.00");
  await page.getByRole("button", { name: "Preview edits" }).click();
  const preview = panel.getByRole("region", {
    name: "Proposed profile changes",
  });
  await preview.getByRole("cell", { name: "12.25", exact: true }).waitFor();
  await preview.getByRole("cell", { name: "4.25", exact: true }).waitFor();
  await page.getByRole("button", { name: "Apply preview" }).click();
  await preview.waitFor({ state: "hidden" });

  await page.getByRole("tab", { name: "Raw View" }).click();
  const xml = page.locator(".cm-content");
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

async function verify(name) {
  const browser = await playwright[name].launch({ timeout: 15_000 });
  const server = await startServer();
  try {
    const context = await browser.newContext({ acceptDownloads: true });
    await simulateUnavailableBank(context);
    await context.addInitScript(() => {
      const BrowserWorker = globalThis.Worker;
      globalThis.Worker = class extends BrowserWorker {
        constructor(url, options) {
          super(url, options);
          globalThis.addEventListener(
            "starbank-test-stop-worker",
            () => {
              this.dispatchEvent(new Event("error"));
            },
            { once: true },
          );
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
    const windowedPage = await context.newPage();
    try {
      await windowedPage.setViewportSize(viewport);
      await windowedPage.goto(server.origin);
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

    const page = await context.newPage();
    await page.goto(server.origin);
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

    await value.fill("1,000,000");
    await page.getByRole("button", { name: "Apply edits" }).click();
    await page
      .getByRole("status")
      .filter({ hasText: "Unsaved changes" })
      .waitFor();
    assert.equal(await value.inputValue(), "1,000,000");
    await downloadOpeningBackup(page, initial);
    await downloadCopy(page, edited, "Control+s");
    await page.keyboard.press("Escape");

    await page.getByRole("tab", { name: "Raw View" }).click();
    const raw = page.locator(".cm-content");
    await raw.waitFor();
    assert.match(await raw.getAttribute("class"), /cm-lineWrapping/);
    assert.match(await raw.textContent(), /int="1000000"/u);
    await raw.fill("<Bank>");
    await page.keyboard.press("Meta+s");
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
    await raw.fill(rawEdited);
    await page.getByRole("button", { name: "Apply XML" }).click();
    await page
      .getByRole("button", { name: "Apply XML" })
      .waitFor({ state: "hidden" });
    await page.getByRole("tab", { name: "Raw View" }).focus();
    await page.keyboard.press("Meta+z");
    await raw.filter({ hasText: "<!-- synthetic -->" }).waitFor();
    await page.keyboard.press("Control+Shift+z");
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

    await page.getByRole("button", { name: "More save options" }).click();
    await page.getByRole("menuitem", { name: "Copy bank XML" }).click();
    dialog = page.getByRole("dialog", { name: "Copy bank XML" });
    const clipboard = await dialog.getByLabel("Prepared bank XML").inputValue();
    assert.equal(
      clipboard.replaceAll("\r\n", "\n"),
      rawEdited.replaceAll("\r\n", "\n"),
    );
    await dialog.getByRole("button", { name: "Close", exact: true }).click();

    page.once("dialog", (confirmation) => confirmation.dismiss());
    await chooseBank("Replacement.SC2Bank", initial);
    await page
      .getByRole("status")
      .filter({ hasText: "The current bank is still open." })
      .waitFor();

    page.once("dialog", (confirmation) => confirmation.accept());
    await chooseBank("Malformed.SC2Bank", "<Bank>");
    await page
      .getByRole("status")
      .filter({ hasText: "The bank XML could not be opened safely." })
      .waitFor();
    await page.getByRole("heading", { name: documentName }).waitFor();
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();

    page.once("dialog", (confirmation) => confirmation.accept());
    await chooseBank(
      "Replacement.SC2Bank",
      initial.replace('int="001"', 'int="002"'),
    );
    await page.getByRole("heading", { name: "Replacement.SC2Bank" }).waitFor();
    assert.equal(await value.inputValue(), "002");
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
