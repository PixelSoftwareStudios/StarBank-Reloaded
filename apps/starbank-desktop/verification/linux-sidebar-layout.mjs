import assert from "node:assert/strict";

const settings = 'section[aria-label="Settings"]';
const splitter = 'hr.sidebar-splitter[aria-label="Resize map list"]';
const manualRequired =
  "Automatic SC2 folder detection is unavailable. Select a folder to continue.";
const defaultWidth = 28;
const minimumWidth = 20;
const maximumWidth = 50;
const savedWidth = 48;
const geometryTolerancePx = 1;
const pointerWidth = 40;
const pointerId = "sidebar-layout-mouse";
const tabLimit = 24;
const tabKey = "\uE004";
const endKey = "\uE010";
const homeKey = "\uE011";
const leftKey = "\uE012";

function sidebarChecks({ evaluate, content }) {
  const readLayout = () =>
    evaluate(
      `const shells = document.querySelectorAll(".starbank-shell");
      if (shells.length !== 1) return null;
      const shell = shells[0];
      const handles = shell.querySelectorAll(arguments[0]);
      const sidebars = shell.querySelectorAll("aside.map-pane");
      if (handles.length !== 1 || sidebars.length !== 1) return null;
      const handle = handles[0];
      const sidebar = sidebars[0];
      const shellStyle = getComputedStyle(shell);
      const sidebarStatuses = Array.from(sidebar.querySelectorAll('[role="status"]'),
        (item) => item.textContent.trim());
      const html = document.documentElement;
      const bounds = handle.getBoundingClientRect();
      return {
        handle: { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height },
        viewport: { width: innerWidth, height: innerHeight },
        enabled: handle.tabIndex === 0 && handle.getAttribute("aria-disabled") !== "true",
        visible: handle.getClientRects().length > 0 && getComputedStyle(handle).visibility === "visible",
        linked: document.getElementById(handle.getAttribute("aria-controls")) === sidebar,
        orientation: handle.getAttribute("aria-orientation"),
        min: handle.getAttribute("aria-valuemin"),
        max: handle.getAttribute("aria-valuemax"),
        now: handle.getAttribute("aria-valuenow"),
        focused: document.activeElement === handle,
        configuredWidth: shellStyle.getPropertyValue("--map-pane-width").trim(),
        shellWidth: shell.getBoundingClientRect().width,
        sidebarWidth: sidebar.getBoundingClientRect().width,
        theme: html.dataset.starbankTheme,
        scheme: getComputedStyle(html).colorScheme,
        density: shell.dataset.density,
        rootless: !sidebar.querySelector("header p[title]") &&
          sidebarStatuses.length === 1 && sidebarStatuses[0] === arguments[1],
      };`,
      splitter,
      manualRequired,
    );
  const readRange = () =>
    evaluate(
      `const panels = document.querySelectorAll(arguments[0]);
      if (panels.length !== 1 || panels[0].querySelector('[role="alert"]')) return null;
      const groups = Array.from(panels[0].querySelectorAll("fieldset"))
        .filter((group) => group.querySelector(":scope > legend")?.textContent.trim() === "Layout");
      if (groups.length !== 1) return null;
      const ranges = Array.from(groups[0].querySelectorAll('input[type="range"]'))
        .filter((input) => input.labels?.[0]?.querySelector(":scope > span")?.textContent.trim() === "Map list width");
      if (ranges.length !== 1) return null;
      const range = ranges[0];
      if (range.matches(":disabled") || range.getClientRects().length === 0) return null;
      return {
        value: range.value,
        valueText: range.getAttribute("aria-valuetext"),
        min: range.min,
        max: range.max,
        step: range.step,
      };`,
      settings,
    );
  async function widthMatches(width, requireFocus = false) {
    const layout = await readLayout();
    return (
      layout?.enabled &&
      layout.visible &&
      layout.linked &&
      layout.orientation === "vertical" &&
      layout.min === String(minimumWidth) &&
      layout.max === String(maximumWidth) &&
      layout.now === String(width) &&
      (!requireFocus || layout.focused) &&
      layout.configuredWidth === `${width}%` &&
      layout.shellWidth > 0 &&
      layout.sidebarWidth > 0 &&
      Math.abs(layout.sidebarWidth - (layout.shellWidth * width) / 100) <=
        geometryTolerancePx &&
      layout.theme === "dark" &&
      layout.scheme === "dark" &&
      layout.density === "comfortable" &&
      layout.rootless
    );
  }
  async function welcomeMatches(width) {
    return (
      (await content(settings)) === null &&
      (await content(".editor-welcome h1")) === "StarBank Reloaded" &&
      (await content('.editor-welcome p[role="status"]')) === "No bank open." &&
      (await widthMatches(width))
    );
  }
  async function confirmedSettings(width) {
    const range = await readRange();
    return (
      range?.value === String(width) &&
      range.valueText === `${width}%` &&
      range.min === String(minimumWidth) &&
      range.max === String(maximumWidth) &&
      range.step === "0.1" &&
      (await widthMatches(width))
    );
  }
  return { readLayout, widthMatches, welcomeMatches, confirmedSettings };
}

export async function verifySidebarLayout({
  command,
  waitFor,
  controls,
  resize,
  expectedWidth = savedWidth,
  assertUnchanged,
}) {
  assert.equal(typeof resize, "boolean");
  assert(Number.isFinite(expectedWidth));
  assert(expectedWidth >= minimumWidth && expectedWidth <= maximumWidth);
  const { click, clickButton, evaluate } = controls;
  const { widthMatches, welcomeMatches, confirmedSettings } =
    sidebarChecks(controls);
  const confirmedWidth = resize ? savedWidth : expectedWidth;
  const press = (value) =>
    command("/actions", "POST", {
      actions: [
        {
          type: "key",
          id: "sidebar-layout-keyboard",
          actions: [
            { type: "keyDown", value },
            { type: "keyUp", value },
          ],
        },
      ],
    });
  async function focusByTab() {
    const focusPath = [];
    for (let step = 0; step <= tabLimit; step++) {
      const active = await evaluate(
        `const active = document.activeElement;
        return {
          splitter: active?.matches(arguments[0]) ?? false,
          name: active?.getAttribute("aria-label") || active?.tagName,
        };`,
        splitter,
      );
      if (active.splitter) return;
      focusPath.push(active.name);
      if (step < tabLimit) await press(tabKey);
    }
    assert.fail(
      `Tab did not reach the sidebar separator. Focus path: ${focusPath.join(", ")}`,
    );
  }

  const openingWidth = resize ? defaultWidth : expectedWidth;
  await waitFor(
    `the unopened welcome view with a ${openingWidth}% sidebar`,
    () => welcomeMatches(openingWidth),
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await assertUnchanged();

  if (resize) {
    await focusByTab();
    assert(await widthMatches(defaultWidth, true));
    for (const [key, width] of [
      [homeKey, minimumWidth],
      [endKey, maximumWidth],
      [leftKey, savedWidth],
    ]) {
      await press(key);
      await waitFor(`the focused separator and ${width}% geometry`, () =>
        widthMatches(width, true),
      );
    }
    await click("aside.map-pane .map-options button", "More…");
  } else {
    await click('button[aria-label="Settings"]', null);
  }

  await waitFor("the confirmed Map list width setting and geometry", () =>
    confirmedSettings(confirmedWidth),
  );
  await assertUnchanged();
  await clickButton("Back to editor");
  await waitFor("the unopened welcome view with the saved sidebar width", () =>
    welcomeMatches(confirmedWidth),
  );
  await assertUnchanged();
  console.log(
    resize
      ? "Linux native sidebar: keyboard resized the sidebar and Settings confirmed 48%."
      : `Linux native sidebar: restart applied ${expectedWidth}% before opening Settings.`,
  );
}

export async function verifySidebarPointerLayout({
  command,
  waitFor,
  controls,
  assertUnchanged,
}) {
  const { click, clickButton } = controls;
  const { readLayout, widthMatches, welcomeMatches, confirmedSettings } =
    sidebarChecks(controls);
  await waitFor("the unopened welcome view with the saved 48% sidebar", () =>
    welcomeMatches(savedWidth),
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await assertUnchanged();

  const layout = await readLayout();
  const { handle, viewport } = layout;
  const x = Math.floor(handle.x + handle.width / 2);
  const y = Math.floor(handle.y + handle.height / 2);
  const delta = Math.round(
    ((pointerWidth - savedWidth) * layout.shellWidth) / 100,
  );
  const destinationX = x + delta;
  const roundedWidth =
    Math.round((savedWidth + (delta / layout.shellWidth) * 100) * 10) / 10;
  assert.equal(
    roundedWidth,
    pointerWidth,
    "The shell width must permit an integer CSS-pixel drag rounding to 40%.",
  );
  assert(x >= handle.x && x < handle.x + handle.width);
  assert(y >= handle.y && y < handle.y + handle.height);
  assert(destinationX >= 0 && destinationX < handle.x);
  assert(x < viewport.width && y >= 0 && y < viewport.height);
  const pointer = (actions) =>
    command("/actions", "POST", {
      actions: [
        {
          type: "pointer",
          id: pointerId,
          parameters: { pointerType: "mouse" },
          actions,
        },
      ],
    });
  const failures = [];
  try {
    await pointer([
      { type: "pointerMove", origin: "viewport", x, y, duration: 0 },
      { type: "pointerDown", button: 0 },
      {
        type: "pointerMove",
        origin: "viewport",
        x: destinationX,
        y,
        duration: 0,
      },
    ]);
    await waitFor("the held pointer preview and 40% sidebar geometry", () =>
      widthMatches(pointerWidth),
    );
  } catch (error) {
    failures.push(error);
  } finally {
    try {
      await pointer([{ type: "pointerUp", button: 0 }]);
    } catch (error) {
      failures.push(error);
    }
  }
  if (failures.length === 1) throw failures[0];
  if (failures.length > 1)
    throw new AggregateError(
      failures,
      "Sidebar drag and pointer release failed.",
    );
  await waitFor("the released pointer and retained 40% sidebar geometry", () =>
    welcomeMatches(pointerWidth),
  );
  await click("aside.map-pane .map-options button", "More…");
  await waitFor("the acknowledged 40% Map list width setting", () =>
    confirmedSettings(pointerWidth),
  );
  await assertUnchanged();
  await clickButton("Back to editor");
  await waitFor("the unopened welcome view after pointer resizing", () =>
    welcomeMatches(pointerWidth),
  );
  await assertUnchanged();
  console.log(
    "Linux native sidebar: captured pointer resized to 40% and Settings confirmed it after release.",
  );
}
