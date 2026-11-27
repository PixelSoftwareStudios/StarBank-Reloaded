import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { lstat } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);
const exportTitle = "Export bank copy";
const chooserTimeout = 10_000;
const commandTimeout = 3_000;
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

/** Submits the unused fixture path in the real GTK chooser on the caller's Xvfb display. */
export async function chooseGtkExportDestination(destination) {
  assert.equal(process.platform, "linux", "GTK export requires Linux.");
  assert(process.env.DISPLAY, "Run the GTK chooser check under Xvfb.");
  assert(isAbsolute(destination), "The export destination must be absolute.");
  assert(
    !["\0", "\t", "\n", "\r"].some((key) => destination.includes(key)),
    "The export path must be typed without keyboard control characters.",
  );
  await assert.rejects(lstat(destination), { code: "ENOENT" });

  const window = await xdotool(
    ["search", "--sync", "--onlyvisible", "--name", `^${exportTitle}$`],
    chooserTimeout,
  );
  assert.match(window, /^[0-9]+$/, "Expected one visible export chooser.");
  assert.equal(await xdotool(["getwindowname", window]), exportTitle);
  await xdotool(["windowfocus", "--sync", window]);

  async function requireFocus() {
    assert.equal(
      await xdotool(["getwindowfocus", "-f"]),
      window,
      "The GTK export chooser lost focus.",
    );
  }

  await requireFocus();
  // Without a window argument, key and type send XTEST events to the focused chooser.
  await xdotool(["key", "--clearmodifiers", "ctrl+l", "ctrl+a"]);
  await xdotool(["type", "--clearmodifiers", "--", destination]);
  await requireFocus();
  await xdotool(["key", "--clearmodifiers", "Return"]);
}
