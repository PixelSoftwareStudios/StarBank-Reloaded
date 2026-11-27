import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const loopback = "127.0.0.1";
const commandTimeout = 5_000;
const startupTimeout = 25_000;
const uiTimeout = 10_000;
const cleanupTimeout = 2_000;
const pollInterval = 100;
const failureItemLimit = 8;
const failureTextLimit = 1_000;
const elementKey = "element-6066-11e4-a52e-4f735466cecf";

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

async function verifyEditor(command, first, second) {
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
  const findControl = (selector, name) =>
    evaluate(
      `
      const [selector, name] = arguments;
      return Array.from(document.querySelectorAll(selector)).find((element) =>
        element.getClientRects().length > 0 && !element.disabled &&
        (name === null || element.textContent.trim() === name)
      ) ?? null;
    `,
      selector,
      name,
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

  await waitFor(
    "the packaged welcome screen",
    async () => (await content(".editor-welcome h1")) === "StarBank Reloaded",
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await waitFor(
    "two pending open requests",
    async () => (await queuedName(first)) === `${first} (1 more)`,
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
      (await queuedName(second)) === second &&
      (await findControl(`${queue} button`, "Open")),
  );

  const field = await control(gold, null);
  await command(`/element/${field}/click`, "POST", {});
  await command(`/element/${field}/clear`, "POST", {});
  await command(`/element/${field}/value`, "POST", {
    text: "1,234",
  });
  await waitFor("a staged Table edit and disabled queued Open", () =>
    evaluate(
      `
    const [queue, gold] = arguments;
    const open = Array.from(document.querySelectorAll(queue + " button"))
      .find((button) => button.textContent.trim() === "Open");
    const field = document.querySelector(gold);
    return open?.disabled === true && field?.value === "1,234" &&
      field.hasAttribute("data-table-draft");
  `,
      queue,
      gold,
    ),
  );
  assert.equal(await queuedName(second), second);

  await clickButton("Apply edits");
  await waitFor(
    "the accepted, dirty edit",
    async () =>
      (await content(".editor-draft-actions")) === null &&
      (await content(".editor-dirty-status")) ===
        "Unsaved changes in 1 section",
  );
  await clickTab("Raw View");
  await waitFor("normalized value in the native Raw projection", async () =>
    (await content(xml))?.includes('int="1234"'),
  );
  await clickButton("Undo");
  await waitFor(
    "the original clean revision",
    async () =>
      (await content(xml))?.includes('int="001"') &&
      (await content(".editor-dirty-status")) === null,
  );
  await click(`${queue} button`, "Open");
  await waitFor(
    "the second native document",
    async () => (await content(".document-editor h1")) === second,
  );
  await clickTab("Table View");
  await waitFor(
    "the second native Table projection and empty queue",
    async () =>
      (await tableValue()) === "007" && (await content(queue)) === null,
  );
}

function bank(value) {
  return Buffer.from(
    `<Bank version="1"><Section name="Player"><Key name="Gold"><Value int="${value}"/></Key></Section></Bank>`,
  );
}

async function run(application) {
  const scratch = await mkdtemp(join(tmpdir(), "starbank-linux-webview-"));
  let driver;
  let session;
  let verificationError;
  try {
    const environment = { ...process.env };
    for (const directory of ["CONFIG", "CACHE", "DATA"]) {
      const path = join(scratch, directory.toLowerCase());
      await mkdir(path);
      environment[`XDG_${directory}_HOME`] = path;
    }
    const first = "First.SC2Bank";
    const second = "Second.SC2Bank";
    const inputs = [
      [first, bank("001")],
      [second, bank("007")],
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
    await verifyEditor(
      (path, method, body) =>
        request(driver.origin, session + path, method, body),
      first,
      second,
    );
    for (const [name, bytes] of inputs)
      assert.deepEqual(await readFile(join(scratch, name)), bytes);
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
    "Linux native open, Table draft guard, Raw adoption, undo, and queued reopen passed.",
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
  3,
  "Pass the packaged starbank-desktop executable path.",
);
await run(resolve(process.argv[2]));
