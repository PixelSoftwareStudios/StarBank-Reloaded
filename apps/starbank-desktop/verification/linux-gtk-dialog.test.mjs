import assert from "node:assert/strict";
import { test } from "node:test";
import { waitForWindowGone } from "./linux-gtk-dialog.mjs";

const title = "StarBank Reloaded";
const args = ["search", "--name", "^StarBank Reloaded$"];
const badWindowStderr =
  "X Error of failed request:  BadWindow (invalid Window parameter)\n" +
  "  Major opcode of failed request:  20 (X_GetProperty)\n" +
  "  Resource id in failed request:  0x200003\n" +
  "  Serial number of failed request:  21\n" +
  "  Current serial number in output stream:  21\n";

function searchExit(stderr = "", overrides = {}) {
  return Object.assign(new Error("xdotool exited"), {
    cmd: ["xdotool", ...args].join(" "),
    code: 1,
    killed: false,
    signal: null,
    stdout: "",
    stderr,
    ...overrides,
  });
}

function virtualTime(search) {
  let elapsed = 0;
  const paused = [];
  function advance(milliseconds) {
    elapsed += milliseconds;
  }
  return {
    options: {
      now: () => elapsed,
      pause: async (milliseconds) => {
        paused.push(milliseconds);
        advance(milliseconds);
      },
      search: async (searchArgs, timeout) =>
        search(searchArgs, timeout, advance),
    },
    paused,
  };
}

test("a title-search BadWindow needs a later clean no-match result", async () => {
  let attempts = 0;
  const outcomes = [searchExit(badWindowStderr), "2097155", searchExit()];
  const clock = virtualTime((searchArgs) => {
    assert.deepEqual(searchArgs, args);
    const outcome = outcomes[attempts++];
    if (outcome instanceof Error) throw outcome;
    return outcome;
  });
  await waitForWindowGone(title, clock.options);
  assert.equal(attempts, 3);
  assert.deepEqual(clock.paused, [100, 100]);
});

test("a clean no-match result is immediate and preserves visible exact-title search", async () => {
  let attempts = 0;
  const clock = virtualTime((searchArgs) => {
    attempts += 1;
    assert.deepEqual(searchArgs, [
      "search",
      "--onlyvisible",
      "--name",
      "^Close\\?$",
    ]);
    throw searchExit("", {
      cmd: "xdotool search --onlyvisible --name ^Close\\?$",
    });
  });
  await waitForWindowGone("Close?", { ...clock.options, visibleOnly: true });
  assert.equal(attempts, 1);
  assert.deepEqual(clock.paused, []);
});

test("repeated BadWindow results stop within the original deadline", async () => {
  const timeouts = [];
  const clock = virtualTime((searchArgs, timeout, advance) => {
    assert.deepEqual(searchArgs, args);
    timeouts.push(timeout);
    advance(timeout + (timeouts.length === 1 ? 0.5 : 0));
    throw searchExit(badWindowStderr);
  });
  await assert.rejects(waitForWindowGone(title, clock.options), {
    message: `The ${title} window did not close.`,
  });
  assert.deepEqual(timeouts, [3_000, 3_000, 3_000, 699]);
  assert.deepEqual(clock.paused, [100, 100, 100]);
  assert.equal(clock.options.now(), 9_999.5);
});

test("a clean no-match at or after the deadline does not confirm closure", async () => {
  for (const elapsed of [10_000, 10_001]) {
    let attempts = 0;
    const clock = virtualTime((_searchArgs, _timeout, advance) => {
      attempts += 1;
      advance(elapsed);
      throw searchExit();
    });
    await assert.rejects(waitForWindowGone(title, clock.options), {
      message: `The ${title} window did not close.`,
    });
    assert.equal(attempts, 1);
    assert.deepEqual(clock.paused, []);
  }
});

test("unrelated commands, errors and incomplete results remain fatal", async () => {
  const failures = [
    searchExit(badWindowStderr, { cmd: "xdotool getwindowname 2097155" }),
    searchExit(badWindowStderr, { code: 2 }),
    searchExit(badWindowStderr, { killed: true, signal: "SIGKILL" }),
    searchExit("", { killed: true, signal: "SIGKILL" }),
    searchExit(badWindowStderr, { stdout: "2097155\n" }),
    searchExit(badWindowStderr.replace("BadWindow", "BadMatch")),
    searchExit(
      badWindowStderr.replace("20 (X_GetProperty)", "15 (X_QueryTree)"),
    ),
    searchExit(badWindowStderr.replace("0x200003", "invalid")),
    searchExit(`${badWindowStderr}unexpected stderr\n`),
    searchExit("unterminated diagnostic"),
    searchExit(undefined, { stderr: undefined }),
  ];
  for (const failure of failures) {
    let attempts = 0;
    const clock = virtualTime(() => {
      attempts += 1;
      throw failure;
    });
    await assert.rejects(
      waitForWindowGone(title, clock.options),
      (error) => error === failure,
    );
    assert.equal(attempts, 1);
    assert.deepEqual(clock.paused, []);
  }
});
