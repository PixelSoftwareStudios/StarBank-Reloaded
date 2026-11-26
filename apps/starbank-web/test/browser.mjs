import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

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

async function downloadCopy(page, expected) {
  await page.getByRole("button", { name: "Save As…" }).click();
  const dialog = page.getByRole("dialog", { name: "Save a bank copy" });
  await dialog.getByRole("button", { name: "Prepare download" }).click();
  const download = page.waitForEvent("download");
  await dialog.getByRole("link", { name: `Download ${documentName}` }).click();
  const copy = await download;
  assert.equal(copy.suggestedFilename(), documentName);
  assert.deepEqual(await readFile(await copy.path()), Buffer.from(expected));
  return dialog;
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

async function verify(name) {
  const browser = await playwright[name].launch({ timeout: 15_000 });
  const server = await startServer();
  try {
    const context = await browser.newContext({ acceptDownloads: true });
    await context.addInitScript(() => {
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
    context.on("request", (request) => requests.push(request.url()));
    const page = await context.newPage();
    page.setDefaultTimeout(10_000);
    page.on("pageerror", (error) => browserErrors.push(error.message));
    await page.goto(server.origin);
    await page.getByRole("button", { name: "Open Bank…" }).waitFor();
    await verifyPreferences(page);
    assert(
      server.requests.some(([, path]) => path.endsWith(".wasm")),
      `${name}: WASM was not fetched`,
    );
    await server.stop();
    const assetRequests = requests.length;

    const fileInput = page.getByLabel("Bank file", { exact: true });
    const chooseBank = async (fileName, xml) =>
      fileInput.setInputFiles({
        name: fileName,
        mimeType: "application/octet-stream",
        buffer: Buffer.from(xml),
      });
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
    await downloadCopy(page, edited);
    await page.keyboard.press("Escape");

    await page.getByRole("tab", { name: "Raw View" }).click();
    const raw = page.locator(".cm-content");
    await raw.waitFor();
    assert.match(await raw.getAttribute("class"), /cm-lineWrapping/);
    assert.match(await raw.textContent(), /int="1000000"/u);
    await raw.fill("<Bank>");
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
    await page.getByRole("button", { name: "Undo" }).click();
    await raw.filter({ hasText: "<!-- synthetic -->" }).waitFor();
    await page.getByRole("button", { name: "Redo" }).click();
    await raw.filter({ hasText: "<!-- raw draft applied -->" }).waitFor();

    let dialog = await downloadCopy(page, rawEdited);

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

    assert.deepEqual(browserErrors, []);
    const networkUrls = requests.filter((url) => url.startsWith("http"));
    assert(networkUrls.every((url) => new URL(url).origin === server.origin));
    assert(
      requests.slice(assetRequests).every((url) => url.startsWith("blob:")),
    );
    console.log(
      `${name} ${browser.version()} passed offline open, table/XML edits, undo/redo, exact and signed downloads, clipboard fallback, guarded file switching, and persisted settings`,
    );
  } finally {
    await server.stop();
    await browser.close();
  }
}

for (const engine of engines.length ? engines : supported) {
  await verify(engine);
}
