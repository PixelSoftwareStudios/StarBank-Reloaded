import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import {
  cancelGtkDirtyClose,
  cancelGtkDirtyOpen,
  chooseGtkExportDestination,
  discardGtkDirtyClose,
  discardGtkDirtyOpen,
  requestGtkWindowClose,
  saveGtkDirtyOpenAs,
} from "./linux-gtk-dialog.mjs";

const loopback = "127.0.0.1";
const commandTimeout = 5_000;
const startupTimeout = 25_000;
const uiTimeout = 10_000;
const cleanupTimeout = 2_000;
const pollInterval = 100;
const failureItemLimit = 8;
const failureTextLimit = 1_000;
const elementKey = "element-6066-11e4-a52e-4f735466cecf";
const controlKey = "\uE009";
const backspaceKey = "\uE003";

async function reservePort() {
  const socket = createServer();
  await new Promise((ready, reject) => {
    socket.once("error", reject);
    socket.listen(0, loopback, ready);
  });
  return {
    port: socket.address().port,
    release: () =>
      new Promise((done, reject) => {
        socket.close((error) => (error ? reject(error) : done()));
      }),
  };
}

async function startDriver(environment) {
  const proxy = await reservePort();
  const native = await reservePort().catch(async (error) => {
    await proxy.release();
    throw error;
  });
  await Promise.all([proxy.release(), native.release()]);
  const child = spawn(
    "tauri-driver",
    [
      `--port=${proxy.port}`,
      `--native-port=${native.port}`,
      `--native-host=${loopback}`,
    ],
    { detached: true, env: environment, stdio: "inherit" },
  );
  let exitReason;
  const exited = new Promise((done) => {
    function recordExit(reason) {
      exitReason = reason;
      done(reason);
    }
    child.once("exit", (code, signal) => recordExit(`exit ${code ?? signal}`));
    child.once("error", (error) => recordExit(error.message));
  });
  return {
    child,
    exited,
    origin: `http://${loopback}:${proxy.port}`,
    get exitReason() {
      return exitReason;
    },
  };
}

function signalDriverGroup(child, signal) {
  if (!child.pid) return;
  try {
    process.kill(-child.pid, signal);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

async function stopDriver({ child, exited }) {
  child.kill("SIGTERM");
  const stopped = await Promise.race([
    exited,
    delay(cleanupTimeout, undefined, { ref: false }),
  ]);
  // Stop any group member left behind if the wrapper or session exited early.
  signalDriverGroup(child, "SIGKILL");
  if (stopped === undefined) {
    const killed = await Promise.race([exited, delay(cleanupTimeout)]);
    assert(killed !== undefined, "The owned WebDriver process did not exit.");
  }
}

async function request(origin, path, method = "GET", body, timeout) {
  const response = await fetch(`${origin}${path}`, {
    method,
    headers:
      body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeout ?? commandTimeout),
  });
  const { value } = await response.json();
  if (!response.ok || value?.error) {
    throw new Error(`${method} ${path}: ${value?.message ?? response.status}`);
  }
  return value;
}

async function waitFor(description, read, timeout = uiTimeout) {
  const deadline = performance.now() + timeout;
  do {
    const result = await read();
    if (result) return result;
    await delay(pollInterval);
  } while (performance.now() < deadline);
  throw new Error(`Timed out waiting for ${description}.`);
}

async function waitForDriver(driver) {
  await waitFor(
    "tauri-driver and WebKitWebDriver to be ready",
    async () => {
      if (driver.exitReason) {
        throw new Error(
          `tauri-driver stopped during startup: ${driver.exitReason}`,
        );
      }
      try {
        return (await request(driver.origin, "/status", "GET", undefined, 500))
          .ready;
      } catch {
        return false;
      }
    },
    startupTimeout,
  );
}

async function reportFailureState(origin, session) {
  try {
    const state = await request(
      origin,
      `${session}/execute/sync`,
      "POST",
      {
        script: `const [itemLimit, textLimit] = arguments;
        return {
          url: location.href.slice(0, textLimit),
          headings: Array.from(document.querySelectorAll("h1")).slice(0, itemLimit)
            .map((node) => node.textContent.slice(0, textLimit)),
          statuses: Array.from(document.querySelectorAll('[role="status"]')).slice(0, itemLimit)
            .map((node) => node.textContent.slice(0, textLimit)),
          fields: Array.from(document.querySelectorAll('table input[type="text"]')).slice(0, itemLimit)
            .map((node) => ({label: node.getAttribute("aria-label"), value: node.value.slice(0, textLimit),
              readOnly: node.readOnly, invalid: node.getAttribute("aria-invalid")})),
          queue: document.querySelector('aside[aria-label="File open requests"]')?.textContent.slice(0, textLimit),
          focused: document.activeElement?.getAttribute("aria-label"),
          documentFocused: document.hasFocus()
        };`,
        args: [failureItemLimit, failureTextLimit],
      },
      cleanupTimeout,
    );
    console.error("Synthetic native UI failure state:", JSON.stringify(state));
  } catch {
    console.error("Synthetic native UI failure state was unavailable.");
  }
}

const queue = 'aside[aria-label="File open requests"]';
const gold = 'table input[aria-label="Gold value"]';
const xml = '[aria-label="Bank XML"]';
const settingsCheckbox =
  'section[aria-label="Settings"] input[type="checkbox"]';
const autoResignLabel = "Auto re-sign banks on save";
const dirtyStatus = "Unsaved changes in 1 section";
const clipboardStatus = "Copied bank XML.";
const clipboardDialogTitle = "Copy bank XML";
const preparedXmlField = 'textarea[aria-label="Prepared bank XML"]';

async function verifyEditor(
  command,
  {
    first,
    second,
    third,
    assertInputsUnchanged,
    exportCopy,
    saveCopy,
    closeSender,
  },
) {
  const evaluate = (script, ...args) =>
    command("/execute/sync", "POST", { script, args });
  const content = (selector) =>
    evaluate(
      "return document.querySelector(arguments[0])?.textContent.trim() ?? null;",
      selector,
    );
  const tableValue = () =>
    evaluate(
      "return document.querySelector(arguments[0])?.value ?? null;",
      gold,
    );
  const findControl = (selector, name, checked = null) =>
    evaluate(
      `
      const [selector, name, checked] = arguments;
      return Array.from(document.querySelectorAll(selector)).find((element) =>
        element.getClientRects().length > 0 && !element.matches(":disabled") &&
        element.getAttribute("aria-disabled") !== "true" &&
        !element.hasAttribute("data-disabled") &&
        (name === null || (element.labels?.[0]?.textContent ?? element.textContent).trim() === name) &&
        (checked === null || element.checked === checked)
      ) ?? null;
    `,
      selector,
      name,
      checked,
    );
  async function control(selector, name) {
    const element = await waitFor(name ?? selector, () =>
      findControl(selector, name),
    );
    assert.equal(
      typeof element[elementKey],
      "string",
      "Missing WebDriver element ID",
    );
    return element[elementKey];
  }
  async function click(selector, name) {
    await command(
      `/element/${await control(selector, name)}/click`,
      "POST",
      {},
    );
  }
  const clickButton = (name) => click("button", name);
  const clickTab = (name) => click('button[role="tab"]', name);
  const queuedName = (name) => content(`${queue} p[title="${name}"]`);
  const secondQueued = `${second} (1 more)`;
  const dirtyBankAndQueueRetained = async () =>
    (await content(".document-editor h1")) === first &&
    (await content(".editor-dirty-status")) === dirtyStatus &&
    (await content(xml))?.includes('int="1234"') &&
    (await queuedName(second)) === secondQueued;

  async function verifyCopy() {
    const observeCopy = () =>
      evaluate(
        `const [success, fieldSelector] = arguments;
        const status = document.querySelector(".editor-status");
        const actionText = Array.from(status?.childNodes ?? [])
          .filter((node) => node.nodeType === Node.TEXT_NODE)
          .map((node) => node.textContent).join("").trim();
        const dialogs = document.querySelectorAll("dialog[open]");
        const dialog = dialogs[0];
        const title = document.getElementById(dialog?.getAttribute("aria-labelledby") ?? "");
        const field = dialog?.querySelector(fieldSelector);
        return {
          copied: actionText === success,
          dialogCount: dialogs.length,
          title: title?.textContent.trim() ?? null,
          instructions: dialog?.querySelector('[role="status"]')?.textContent.trim() ?? null,
          prepared: field?.getClientRects().length
            ? { readOnly: field.readOnly, value: field.value }
            : null,
        };`,
        clipboardStatus,
        preparedXmlField,
      );
    const before = await observeCopy();
    assert.equal(before.copied, false, "Copy success must be fresh.");
    assert.equal(before.dialogCount, 0, "Copy must start without a modal.");
    await click('button[aria-label="More save options"]', null);
    await click('[role="menuitem"]', "Copy bank XML to clipboard");
    const observed = await waitFor("a native Copy outcome", async () => {
      const outcome = await observeCopy();
      return outcome.copied || outcome.dialogCount > 0 ? outcome : null;
    });
    if (observed.copied) {
      assert.equal(
        observed.dialogCount,
        0,
        "Copy reported success and opened a dialog.",
      );
    } else {
      assert.equal(observed.dialogCount, 1);
      assert.equal(observed.title, clipboardDialogTitle);
      assert.equal(observed.instructions, "Select and copy the prepared XML.");
      assert.deepEqual(observed.prepared, {
        readOnly: true,
        value: bank("1234").toString("utf8"),
      });
      await click("dialog[open] button", "Close");
      await waitFor("removal of the prepared XML dialog", () =>
        evaluate(
          `return !Array.from(document.querySelectorAll("dialog")).some((dialog) =>
          document.getElementById(dialog.getAttribute("aria-labelledby"))?.textContent.trim() === arguments[0]
        );`,
          clipboardDialogTitle,
        ),
      );
    }
    await waitFor(
      "the dirty bank and queue after Copy",
      dirtyBankAndQueueRetained,
    );
    await assertInputsUnchanged();
    console.log(
      `Linux native Copy: ${observed.copied ? "WebView clipboard API resolved" : "exact prepared XML shown in the read-only fallback"}.`,
    );
  }

  async function stageGold(value, { hasQueuedOpen = true } = {}) {
    const field = await control(gold, null);
    await command(`/element/${field}/click`, "POST", {});
    await command("/actions", "POST", {
      actions: [
        {
          type: "key",
          id: "table-keyboard",
          actions: [
            { type: "keyDown", value: controlKey },
            { type: "keyDown", value: "a" },
            { type: "keyUp", value: "a" },
            { type: "keyUp", value: controlKey },
            { type: "keyDown", value: backspaceKey },
            { type: "keyUp", value: backspaceKey },
          ],
        },
      ],
    });
    await waitFor("the focused Table draft to be cleared", () =>
      evaluate(
        `const field = document.querySelector(arguments[0]);
        return document.activeElement === field && field?.value === "" &&
          field.hasAttribute("data-table-draft");`,
        gold,
      ),
    );
    await command(`/element/${field}/value`, "POST", { text: value });
    await waitFor("a staged Table edit with guarded navigation", () =>
      evaluate(
        `const [queue, gold, value, hasQueuedOpen] = arguments;
        const open = Array.from(document.querySelectorAll(queue + " button"))
          .find((button) => button.textContent.trim() === "Open");
        const field = document.querySelector(gold);
        const navigationGuarded = hasQueuedOpen ? open?.disabled === true : open === undefined;
        return navigationGuarded && field?.value === value &&
          field.hasAttribute("data-table-draft");`,
        queue,
        gold,
        value,
        hasQueuedOpen,
      ),
    );
  }

  async function applyGold() {
    await clickButton("Apply edits");
    await waitFor(
      "the accepted, dirty edit",
      async () =>
        (await content(".editor-draft-actions")) === null &&
        (await content(".editor-dirty-status")) === dirtyStatus,
    );
  }

  await waitFor(
    "the packaged welcome screen",
    async () => (await content(".editor-welcome h1")) === "StarBank Reloaded",
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await waitFor(
    "three pending open requests",
    async () => (await queuedName(first)) === `${first} (2 more)`,
  );
  await click(`${queue} button`, "Open");
  await waitFor(
    "the first native Table projection",
    async () =>
      (await content(".document-editor h1")) === first &&
      (await tableValue()) === "001",
  );
  await waitFor(
    "the second queued bank before editing",
    async () =>
      (await queuedName(second)) === secondQueued &&
      (await findControl(`${queue} button`, "Open")),
  );

  await stageGold("1,234");
  assert.equal(await queuedName(second), secondQueued);
  await applyGold();
  await clickTab("Raw View");
  await waitFor("normalized value in the native Raw projection", async () =>
    (await content(xml))?.includes('int="1234"'),
  );
  await verifyCopy();
  await click('button[aria-label="More save options"]', null);
  await click('[role="menuitem"]', "Export copy…");
  await exportCopy();
  await waitFor(
    "a verified export receipt with the dirty document retained",
    async () =>
      (await content(".editor-status"))?.includes("Exported Copy.sc2bank.") &&
      (await dirtyBankAndQueueRetained()),
  );
  await click(`${queue} button`, "Open");
  await cancelGtkDirtyOpen();
  await waitFor(
    "the dirty document and enabled queued Open after native Cancel",
    async () =>
      (await dirtyBankAndQueueRetained()) &&
      (await findControl(`${queue} button`, "Open")),
  );
  await clickButton("Undo");
  await waitFor(
    "the original clean revision",
    async () =>
      (await content(xml))?.includes('int="001"') &&
      (await content(".editor-dirty-status")) === null,
  );
  await clickButton("Redo");
  await waitFor(
    "the dirty revision restored by Redo",
    dirtyBankAndQueueRetained,
  );
  await click(`${queue} button`, "Open");
  await discardGtkDirtyOpen();
  await waitFor(
    "the second native document after Discard",
    async () => (await content(".document-editor h1")) === second,
  );
  await clickTab("Table View");
  await waitFor(
    "the second clean native Table projection with the third bank queued",
    async () =>
      (await tableValue()) === "007" &&
      (await content(".editor-dirty-status")) === null &&
      (await queuedName(third)) === third,
  );

  await click('button[aria-label="Settings"]', null);
  await waitFor("the enabled auto re-sign setting with its default value", () =>
    findControl(settingsCheckbox, autoResignLabel, true),
  );
  await click(settingsCheckbox, autoResignLabel);
  await waitFor("the confirmed unchecked auto re-sign setting", () =>
    findControl(settingsCheckbox, autoResignLabel, false),
  );
  await clickButton("Back to editor");
  await waitFor(
    "the second Table projection after Settings",
    async () =>
      (await content(".document-editor h1")) === second &&
      (await tableValue()) === "007" &&
      (await queuedName(third)) === third,
  );

  await stageGold("2,345");
  await applyGold();
  await clickTab("Raw View");
  await waitFor("normalized second bank before Save As", async () =>
    (await content(xml))?.includes('int="2345"'),
  );
  await click(`${queue} button`, "Open");
  await saveCopy();
  await waitFor(
    "the third native document after Save As",
    async () => (await content(".document-editor h1")) === third,
  );
  await clickTab("Table View");
  await waitFor(
    "the third clean native Table projection and empty queue",
    async () =>
      (await tableValue()) === "008" &&
      (await content(".editor-dirty-status")) === null &&
      (await content(queue)) === null,
  );
  assert.equal(
    await evaluate(`return Array.from(document.querySelectorAll(".document-editor button"))
    .some((button) => ["Undo", "Redo"].includes(button.textContent.trim()));`),
    false,
  );
  await stageGold("3,456", { hasQueuedOpen: false });
  await applyGold();
  await clickTab("Raw View");
  await waitFor("the third dirty revision before closing", async () =>
    (await content(xml))?.includes('int="3456"'),
  );
  const mainWindow = await requestGtkWindowClose(closeSender);
  await cancelGtkDirtyClose();
  await waitFor(
    "the usable dirty editor after native close Cancel",
    async () =>
      (await content(".document-editor h1")) === third &&
      (await content(".editor-dirty-status")) === dirtyStatus &&
      (await content(xml))?.includes('int="3456"') &&
      (await findControl("button", "Undo")),
  );
  await requestGtkWindowClose(closeSender, mainWindow);
  await discardGtkDirtyClose();
}

function bank(value) {
  return Buffer.from(
    `<Bank version="1"><Section name="Player"><Key name="Gold"><Value int="${value}"/></Key></Section></Bank>`,
  );
}

async function run(application, closeSender) {
  const scratch = await mkdtemp(join(tmpdir(), "starbank-linux-webview-"));
  let driver;
  let session;
  let verificationError;
  try {
    const environment = { ...process.env, GTK_USE_PORTAL: "0" };
    for (const directory of ["CONFIG", "CACHE", "DATA"]) {
      const path = join(scratch, directory.toLowerCase());
      await mkdir(path);
      environment[`XDG_${directory}_HOME`] = path;
    }
    const first = "First.SC2Bank";
    const second = "Second.SC2Bank";
    const third = "Third.SC2Bank";
    const inputs = [
      [first, bank("001")],
      [second, bank("007")],
      [third, bank("008")],
    ];
    for (const [name, bytes] of inputs)
      await writeFile(join(scratch, name), bytes);
    driver = await startDriver(environment);
    await waitForDriver(driver);
    const { sessionId, capabilities } = await request(
      driver.origin,
      "/session",
      "POST",
      {
        capabilities: {
          alwaysMatch: {
            "tauri:options": {
              application,
              args: inputs.map(([name]) => join(scratch, name)),
            },
          },
        },
      },
      startupTimeout,
    );
    assert(
      typeof sessionId === "string" && sessionId.length > 0,
      "Missing WebDriver session ID",
    );
    session = `/session/${encodeURIComponent(sessionId)}`;
    console.log(
      `Linux native WebView: ${capabilities.browserName} ${capabilities.browserVersion}`,
    );
    const exportDestination = join(scratch, "Copy.sc2bank");
    const saveDestination = join(scratch, "Saved.sc2bank");
    const expectedExport = bank("1234");
    async function assertInputsUnchanged() {
      for (const [name, bytes] of inputs)
        assert.deepEqual(await readFile(join(scratch, name)), bytes);
    }
    await verifyEditor(
      (path, method, body) =>
        request(driver.origin, session + path, method, body),
      {
        first,
        second,
        third,
        closeSender,
        assertInputsUnchanged,
        async exportCopy() {
          await chooseGtkExportDestination(exportDestination);
          await waitFor("the exported bank file", async () => {
            try {
              return (await readFile(exportDestination)).equals(expectedExport);
            } catch (error) {
              if (error.code === "ENOENT") return false;
              throw error;
            }
          });
          await assertInputsUnchanged();
        },
        saveCopy: () => saveGtkDirtyOpenAs(saveDestination),
      },
    );
    await assertInputsUnchanged();
    assert.deepEqual(await readFile(exportDestination), expectedExport);
    assert.deepEqual(await readFile(saveDestination), bank("2345"));
  } catch (error) {
    verificationError = error;
    if (driver && session) await reportFailureState(driver.origin, session);
  }
  const cleanupErrors = [];
  for (const cleanup of [
    () =>
      session &&
      request(driver.origin, session, "DELETE", undefined, cleanupTimeout),
    () => driver && stopDriver(driver),
    () => rm(scratch, { recursive: true, force: true }),
  ]) {
    try {
      await cleanup();
    } catch (error) {
      cleanupErrors.push(error);
    }
  }
  if (cleanupErrors.length > 0) {
    const errors = verificationError
      ? [verificationError, ...cleanupErrors]
      : cleanupErrors;
    throw new AggregateError(errors, "Linux native WebView cleanup failed.");
  }
  if (verificationError) throw verificationError;
  console.log(
    "Linux native editing, Copy XML, GTK export, navigation Cancel/Discard/Save As, and window close Cancel/Discard passed.",
  );
}

assert.equal(
  process.platform,
  "linux",
  "Run the native WebView check on Linux under Xvfb.",
);
assert(process.env.DISPLAY, "Start the check with xvfb-run.");
assert.equal(
  process.argv.length,
  4,
  "Pass the packaged starbank-desktop executable and X11 close sender paths.",
);
await run(resolve(process.argv[2]), resolve(process.argv[3]));
