import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { lstat } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";

const run = promisify(execFile);
const exportTitle = "Export bank copy";
const saveAsTitle = "Save Bank As";
const dirtyOpenTitle = "Save changes before opening another bank?";
const dirtyCloseTitle = "Save changes before closing?";
const draftCloseTitle = "Discard changes and close?";
const mainWindowTitle = "StarBank Reloaded";
const dialogTimeout = 10_000;
const commandTimeout = 3_000;
const pollInterval = 100;
const outputLimit = 4_096;

async function xdotool(args, timeout = commandTimeout) {
  const { stdout } = await run("xdotool", args, {
    encoding: "utf8",
    killSignal: "SIGKILL",
    maxBuffer: outputLimit,
    timeout,
  });
  return stdout.trimEnd();
}

function titlePattern(title) {
  return `^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`;
}

async function requireFocus(window) {
  assert.equal(
    await xdotool(["getwindowfocus", "-f"]),
    window,
    "The GTK dialog lost focus.",
  );
}

async function focusDialog(title) {
  assert.equal(process.platform, "linux", "GTK dialog checks require Linux.");
  assert(process.env.DISPLAY, "Run the GTK dialog check under Xvfb.");
  const window = await xdotool(
    ["search", "--sync", "--onlyvisible", "--name", titlePattern(title)],
    dialogTimeout,
  );
  assert.match(window, /^[0-9]+$/, `Expected one visible ${title} dialog.`);
  assert.equal(await xdotool(["getwindowname", window]), title);
  await xdotool(["windowfocus", "--sync", window]);
  await requireFocus(window);
  return window;
}

async function chooseGtkDestination(title, destination) {
  assert(isAbsolute(destination), "The fixture destination must be absolute.");
  assert(
    !["\0", "\t", "\n", "\r"].some((key) => destination.includes(key)),
    "The fixture path must be typed without keyboard control characters.",
  );
  await assert.rejects(lstat(destination), { code: "ENOENT" });

  const window = await focusDialog(title);
  // Without a window argument, key and type send XTEST events to the focused chooser.
  await xdotool(["key", "--clearmodifiers", "ctrl+l", "ctrl+a"]);
  await xdotool(["type", "--clearmodifiers", "--", destination]);
  await requireFocus(window);
  await xdotool(["key", "--clearmodifiers", "Return"]);
}

/** Submits an unused fixture path in the real GTK Export Copy chooser. */
export function chooseGtkExportDestination(destination) {
  return chooseGtkDestination(exportTitle, destination);
}

async function waitForWindowGone(title) {
  const deadline = performance.now() + dialogTimeout;
  do {
    try {
      await xdotool(["search", "--name", titlePattern(title)]);
    } catch (error) {
      // xdotool search exits 1 without output when no window matches.
      if (error.code === 1 && !error.stdout && !error.stderr) return;
      throw error;
    }
    await delay(pollInterval);
  } while (performance.now() < deadline);
  throw new Error(`The ${title} window did not close.`);
}

async function respondToGtkPrompt(title, ...keys) {
  await focusDialog(title);
  await xdotool(["key", "--clearmodifiers", ...keys]);
  await waitForWindowGone(title);
}

/** Cancels the real GTK dirty-document Open confirmation on the caller's Xvfb display. */
export function cancelGtkDirtyOpen() {
  return respondToGtkPrompt(dirtyOpenTitle, "Escape");
}

/** Discards through the real three-button GTK dirty-document confirmation. */
export function discardGtkDirtyOpen() {
  // On Noble's GTK 3.24.41 this RFD dialog focuses its first button, Save.
  // Tab moves to Discard in the Save, Discard, Cancel order and Space activates it.
  return respondToGtkPrompt(dirtyOpenTitle, "Tab", "space");
}

/** Selects Save on the GTK dirty-open prompt and supplies the new Save As path. */
export async function saveGtkDirtyOpenAs(destination) {
  // The exact three-button dialog on Noble's GTK 3.24.41 initially focuses Save.
  await respondToGtkPrompt(dirtyOpenTitle, "space");
  await chooseGtkDestination(saveAsTitle, destination);
}

/** Sends ICCCM WM_DELETE_WINDOW to this fixture's exact main X11 window. */
export async function requestGtkWindowClose(sender, expectedWindow) {
  const window = await focusDialog(mainWindowTitle);
  if (expectedWindow !== undefined) assert.equal(window, expectedWindow);
  await run(sender, [window], {
    encoding: "utf8",
    killSignal: "SIGKILL",
    maxBuffer: outputLimit,
    timeout: commandTimeout,
  });
  return window;
}

export function cancelGtkDirtyClose() {
  return respondToGtkPrompt(dirtyCloseTitle, "Escape");
}

export async function discardGtkDirtyClose() {
  await respondToGtkPrompt(dirtyCloseTitle, "Tab", "space");
  await waitForWindowGone(mainWindowTitle);
}

export function cancelGtkDraftClose() {
  return respondToGtkPrompt(draftCloseTitle, "Escape");
}

export async function discardGtkDraftClose() {
  // GTK 3.24.41's stock OK/Cancel dialog uses _OK in the fixture's C.UTF-8 locale.
  await respondToGtkPrompt(draftCloseTitle, "alt+o");
  await waitForWindowGone(mainWindowTitle);
}

export async function cancelGtkSaveAsOnClose() {
  await respondToGtkPrompt(dirtyCloseTitle, "space");
  await respondToGtkPrompt(saveAsTitle, "Escape");
}

export async function saveGtkDirtyCloseAs(destination) {
  await respondToGtkPrompt(dirtyCloseTitle, "space");
  await chooseGtkDestination(saveAsTitle, destination);
  await waitForWindowGone(mainWindowTitle);
}
