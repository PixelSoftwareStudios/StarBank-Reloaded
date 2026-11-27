import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import playwright from "playwright/package.json" with { type: "json" };

const dist = resolve(
  process.argv[2] ?? fileURLToPath(new URL("../dist", import.meta.url)),
);
const samples = 25;
const warmups = 5;
const viewport = { width: 850, height: 500 };
const distantIndex = 9_000;
const fileInput = 'input[aria-label="Bank file"]:not(:disabled)';
const mime = {
  ".css": "text/css",
  ".html": "text/html",
  ".js": "text/javascript",
  ".wasm": "application/wasm",
};
const fixture = (rows) => ({
  rows,
  bank: Buffer.from(
    `<Bank version="1"><Section name="Fixture">${Array.from(
      { length: rows },
      (_, index) => `<Key name="Row ${index}"><Value int="${index}"/></Key>`,
    ).join("")}</Section></Bank>`,
  ),
});
const fixtures = [fixture(32), fixture(10_000)];
const row = (index) => `input[aria-label="Row ${index} · Value value"]`;
const paths = [
  "/index.html",
  ...(await readdir(join(dist, "assets")))
    .sort()
    .map((name) => `/assets/${name}`),
];
const assets = new Map(
  await Promise.all(
    paths.map(async (path) => [path, await readFile(join(dist, path))]),
  ),
);
const hash = createHash("sha256");
for (const [path, bytes] of assets) hash.update(path).update(bytes);

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
const requests = [];
const server = createServer((request, response) => {
  const pathname = new URL(request.url, "http://127.0.0.1").pathname;
  const path = pathname === "/" ? "/index.html" : pathname;
  const bytes = assets.get(path);
  if (request.method !== "GET" || !bytes) {
    response.writeHead(404).end();
    return;
  }
  requests.push(path);
  response
    .writeHead(200, {
      "Content-Type": mime[extname(path)],
      "Content-Security-Policy": csp,
      "Cache-Control":
        path === "/index.html"
          ? "no-store"
          : "public, max-age=31536000, immutable",
      "Content-Length": bytes.length,
    })
    .end(bytes);
});

await new Promise((done, fail) => {
  server.once("error", fail);
  server.listen(0, "127.0.0.1", done);
});
const origin = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ timeout: 15_000 });

function installWatch({ fileInput }) {
  const values = {};
  const watch = (label, started, done) => {
    const observe = new MutationObserver(check);
    function check() {
      if (!done()) return;
      values[label] = performance.now() - started;
      observe.disconnect();
    }
    observe.observe(document, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true,
    });
    check();
  };
  globalThis.__starbankPerf = { values, watch };
  watch("startup", 0, () => document.querySelector(fileInput) !== null);
}

async function elapsed(page, label) {
  await page.waitForFunction(
    (key) => key in globalThis.__starbankPerf.values,
    label,
  );
  return page.evaluate((key) => globalThis.__starbankPerf.values[key], label);
}

async function load(context) {
  const page = await context.newPage();
  const first = requests.length;
  await page.goto(origin, { waitUntil: "commit" });
  const startup = await elapsed(page, "startup");
  const served = requests
    .slice(first)
    .filter((path) => path.startsWith("/assets/"));
  return {
    page,
    startup,
    assetRequests: served.length,
    wasmRequests: served.filter((path) => path.endsWith(".wasm")).length,
  };
}

async function measureBank(page, { rows, bank }) {
  await page.evaluate(
    ({ input, firstRow }) => {
      function selected(event) {
        if (!event.target.matches(input)) return;
        window.removeEventListener("change", selected, true);
        globalThis.__starbankPerf.watch(
          "open",
          performance.now(),
          () => document.querySelector(firstRow) !== null,
        );
      }
      window.addEventListener("change", selected, true);
    },
    { input: fileInput, firstRow: row(0) },
  );

  await page.getByLabel("Bank file", { exact: true }).setInputFiles({
    name: "Synthetic.SC2Bank",
    mimeType: "application/octet-stream",
    buffer: bank,
  });
  const open = await elapsed(page, "open");
  const table = page.getByRole("table", { name: "Fixture", exact: true });
  const initialMounted = await table
    .locator('tbody tr:not([aria-hidden="true"])')
    .count();
  if (rows === 32) assert.equal(initialMounted, rows);
  else assert(initialMounted > 1 && initialMounted < 80);
  const index = rows > 32 ? distantIndex : 0;
  let scroll = null;
  let distantMounted = null;
  if (index !== 0) {
    await page.evaluate(
      ({ index, targetRow }) => {
        const viewport = document.querySelector(".editor-windowed-table");
        const height = viewport
          .querySelector("tbody tr[data-bank-row-index]")
          .getBoundingClientRect().height;
        globalThis.__starbankPerf.watch(
          "scroll",
          performance.now(),
          () => document.querySelector(targetRow) !== null,
        );
        viewport.scrollTop = index * height;
      },
      { index, targetRow: row(index) },
    );
    scroll = await elapsed(page, "scroll");
    distantMounted = await table
      .locator('tbody tr:not([aria-hidden="true"])')
      .count();
    assert(distantMounted < 80);
  }
  const value = page.locator(row(index));
  await value.focus();
  await page.waitForFunction(
    ({ selector, initial }) =>
      document.querySelector(selector)?.value === initial,
    { selector: row(index), initial: String(index) },
  );
  await page.evaluate((targetRow) => {
    function drafted(event) {
      if (!event.target.matches(targetRow)) return;
      window.removeEventListener("input", drafted, true);
      globalThis.__starbankPerf.watch(
        "draftValidation",
        performance.now(),
        () =>
          [...document.querySelectorAll(".editor-draft-actions button")].some(
            (button) =>
              button.textContent.trim() === "Apply edits" && !button.disabled,
          ),
      );
    }
    window.addEventListener("input", drafted, true);
  }, row(index));
  await value.fill("9,007,199,254,740,993");
  const draftValidation = await elapsed(page, "draftValidation");
  assert.equal(await value.inputValue(), "9,007,199,254,740,993");
  await page.evaluate(
    ({ targetRow, input }) => {
      function clicked(event) {
        if (
          event.target.closest("button")?.textContent.trim() !== "Apply edits"
        )
          return;
        window.removeEventListener("click", clicked, true);
        const sawRevision = () =>
          document.querySelector(targetRow)?.value ===
            "9,007,199,254,740,993" &&
          document.querySelector(input) !== null &&
          document.querySelector(".editor-draft-actions") === null;
        globalThis.__starbankPerf.watch(
          "apply",
          performance.now(),
          sawRevision,
        );
      }
      window.addEventListener("click", clicked, true);
    },
    { targetRow: row(index), input: fileInput },
  );
  await page.getByRole("button", { name: "Apply edits", exact: true }).click();
  const apply = await elapsed(page, "apply");
  assert.equal(await value.inputValue(), "9,007,199,254,740,993");
  return {
    rows,
    bytes: bank.length,
    open,
    scroll,
    draftValidation,
    apply,
    initialMounted,
    distantMounted,
  };
}

try {
  console.log(
    JSON.stringify({
      type: "environment",
      chromium: browser.version(),
      playwright: playwright.version,
      node: process.version,
      viewport,
      samples,
      warmups,
      assetsSha256: hash.digest("hex"),
    }),
  );
  for (let index = -warmups; index < samples; index += 1) {
    const context = await browser.newContext({ viewport });
    await context.addInitScript(installWatch, { fileInput });
    context.setDefaultTimeout(15_000);
    const errors = [];
    const offOrigin = [];
    context.on("page", (page) =>
      page.on("pageerror", (error) => errors.push(error.message)),
    );
    context.on("request", (request) => {
      const url = request.url();
      if (url.startsWith("http") && new URL(url).origin !== origin)
        offOrigin.push(url);
    });
    try {
      const { page: coldPage, ...cold } = await load(context);
      await coldPage.close();
      const { page: warmPage, ...warm } = await load(context);
      const first = fixtures[Math.abs(index) % fixtures.length];
      const second = fixtures.find((entry) => entry !== first);
      const firstBank = await measureBank(warmPage, first);
      await warmPage.close();
      const { page: other } = await load(context);
      const secondBank = await measureBank(other, second);
      await other.close();
      assert.equal(cold.wasmRequests, 1);
      assert.equal(warm.wasmRequests, 0);
      assert(cold.assetRequests > warm.assetRequests);
      assert.deepEqual(errors, []);
      assert.deepEqual(offOrigin, []);
      if (index >= 0)
        console.log(
          JSON.stringify({
            type: "sample",
            index,
            cold,
            warm,
            fixtures: [firstBank, secondBank].sort((a, b) => a.rows - b.rows),
          }),
        );
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
