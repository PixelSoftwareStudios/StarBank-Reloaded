import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { lstat } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";

const run = promisify(execFile);
const exportTitle = "Export bank copy";
const dirtyOpenTitle = "Save changes before opening another bank?";
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

/** Submits the unused fixture path in the real GTK chooser on the caller's Xvfb display. */
export async function chooseGtkExportDestination(destination) {
  assert(isAbsolute(destination), "The export destination must be absolute.");
  assert(
    !["\0", "\t", "\n", "\r"].some((key) => destination.includes(key)),
    "The export path must be typed without keyboard control characters.",
  );
  await assert.rejects(lstat(destination), { code: "ENOENT" });

  const window = await focusDialog(exportTitle);
  // Without a window argument, key and type send XTEST events to the focused chooser.
  await xdotool(["key", "--clearmodifiers", "ctrl+l", "ctrl+a"]);
  await xdotool(["type", "--clearmodifiers", "--", destination]);
  await requireFocus(window);
  await xdotool(["key", "--clearmodifiers", "Return"]);
}

async function respondToGtkDirtyOpen(...keys) {
  await focusDialog(dirtyOpenTitle);
  await xdotool(["key", "--clearmodifiers", ...keys]);

  const deadline = performance.now() + dialogTimeout;
  do {
    try {
      await xdotool(["search", "--name", titlePattern(dirtyOpenTitle)]);
    } catch (error) {
      // xdotool search exits 1 without output when no window matches.
      if (error.code === 1 && !error.stdout && !error.stderr) return;
      throw error;
    }
    await delay(pollInterval);
  } while (performance.now() < deadline);
  throw new Error("The GTK dirty-document Open confirmation did not close.");
}

/** Cancels the real GTK dirty-document Open confirmation on the caller's Xvfb display. */
export function cancelGtkDirtyOpen() {
  return respondToGtkDirtyOpen("Escape");
}

/** Discards through the real three-button GTK dirty-document confirmation. */
export function discardGtkDirtyOpen() {
  // On Noble's GTK 3.24.41 this RFD dialog focuses its first button, Save.
  // Tab moves to Discard in the Save, Discard, Cancel order and Space activates it.
  return respondToGtkDirtyOpen("Tab", "space");
}
