import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

const playwright = await import(
  process.env.STARBANK_TEST_PLAYWRIGHT_MODULE ?? "playwright"
);
const supported = ["chromium", "firefox", "webkit"];
const engines = process.argv.slice(2);
assert(
  engines.every((engine) => supported.includes(engine)),
  "Unknown browser requested",
);

const wide = { width: 850, height: 500 };
const narrow = { width: 390, height: 640 };

const server = await createServer({
  root: fileURLToPath(new URL("../", import.meta.url)),
  configFile: fileURLToPath(new URL("../vite.config.ts", import.meta.url)),
  server: { host: "127.0.0.1", port: 0, watch: null },
});

async function verify(origin, engine) {
  const browser = await playwright[engine].launch();
  try {
    const page = await browser.newPage({ viewport: wide });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error));
    await page.goto(`${origin}/test/fixtures/breadcrumbs.html`);
    const path = page.getByRole("group", { name: "Bank file path" });
    const pathSelector = ".bank-breadcrumbs";
    const atTail = () =>
      page.waitForFunction((selector) => {
        const viewport = document.querySelector(selector);
        if (!viewport?.clientWidth) return false;
        const tail = viewport.scrollWidth - viewport.clientWidth;
        return tail > 0 && Math.abs(tail - viewport.scrollLeft) <= 1;
      }, pathSelector);
    const offset = () => path.evaluate((element) => element.scrollLeft);
    const segments = () =>
      path.evaluate((element) =>
        Array.from(
          element.querySelectorAll("li bdi"),
          (part) => part.textContent,
        ),
      );

    await atTail();
    assert.deepEqual(await segments(), [
      "/",
      "Synthetic account",
      "A very long directory containing locally selected account folders",
      "Synthetic account",
      "A path with  multiple  spaces",
      "Files chosen in the native picker belong under this test directory",
      "An assorted custom mission bank.SC2Bank",
    ]);
    assert.equal(
      await path.evaluate((element) => {
        const current = element.querySelector('[aria-current="location"]');
        const filename = current.getBoundingClientRect();
        const viewport = element.getBoundingClientRect();
        return (
          filename.left >= viewport.left && filename.right <= viewport.right
        );
      }),
      true,
      "The complete ordinary filename must be visible at the tail",
    );
    assert.equal(
      await path.evaluate(
        (element) =>
          getComputedStyle(element).scrollbarWidth === "none" ||
          getComputedStyle(element, "::-webkit-scrollbar").display === "none",
      ),
      true,
      "The breadcrumb hides its horizontal scrollbar",
    );
    const reveal = page.getByRole("button", { name: "Open in Folder" });
    const iconWidth = (await reveal.boundingBox()).width;

    await page.setViewportSize(narrow);
    await atTail();
    assert.ok(Math.abs((await reveal.boundingBox()).width - iconWidth) < 1);
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth),
      narrow.width,
    );

    await path.press("Home");
    assert.equal(await offset(), 0);
    assert.equal(
      await path.evaluate((element) => element.matches(":focus-visible")),
      true,
    );
    assert.equal(
      await path.evaluate((element) => getComputedStyle(element).outlineStyle),
      "solid",
    );
    await path.press("ArrowRight");
    const manualOffset = await offset();
    assert.ok(manualOffset > 0);
    await page.waitForFunction(
      (selector) => document.querySelector(selector).scrollLeft > 0,
      pathSelector,
    );

    await page.setViewportSize(wide);
    await path.evaluate(
      () =>
        new Promise((done) =>
          requestAnimationFrame(() => requestAnimationFrame(done)),
        ),
    );
    assert.equal(
      await offset(),
      manualOffset,
      "Resize must preserve the manual position",
    );

    await path.press("End");
    await atTail();
    await page.setViewportSize(narrow);
    await atTail();

    await path.press("Home");
    await page.getByRole("button", { name: "Open Bank…" }).click();
    await atTail();
    assert.equal(
      (await segments()).at(-1),
      `A ${"long ".repeat(27)}bank.SC2Bank`,
    );
    assert.equal(
      await path.evaluate(
        (element) =>
          element
            .querySelector('[aria-current="location"]')
            .getBoundingClientRect().width > element.clientWidth,
      ),
      true,
      "An overlong filename remains available through horizontal scrolling",
    );

    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const settings = page.getByRole("region", { name: "Settings" });
    await settings.getByLabel("Show path breadcrumbs").uncheck();
    const back = settings.getByRole("button", { name: "Back to editor" });
    await back.click();
    assert.equal(await path.count(), 0);
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await settings.getByLabel("Show path breadcrumbs").check();
    await back.click();
    await atTail();

    assert.deepEqual(errors, []);
    console.log(
      `${engine} ${browser.version()} passed breadcrumb layout and navigation`,
    );
  } finally {
    await browser.close();
  }
}

try {
  await server.listen();
  const address = server.httpServer.address();
  for (const engine of engines.length ? engines : supported) {
    await verify(`http://127.0.0.1:${address.port}`, engine);
  }
} finally {
  await server.close();
}
