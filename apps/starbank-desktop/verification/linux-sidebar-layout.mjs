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
const tabLimit = 24;
const tabKey = "\uE004";
const endKey = "\uE010";
const homeKey = "\uE011";
const leftKey = "\uE012";

export async function verifySidebarLayout({
  command,
  waitFor,
  controls,
  resize,
  assertUnchanged,
}) {
  assert.equal(typeof resize, "boolean");
  const { click, clickButton, content, evaluate } = controls;
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
      return {
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

  const openingWidth = resize ? defaultWidth : savedWidth;
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

  await waitFor(
    "the confirmed Map list width setting and geometry",
    async () => {
      const range = await readRange();
      return (
        range?.value === String(savedWidth) &&
        range.valueText === `${savedWidth}%` &&
        range.min === String(minimumWidth) &&
        range.max === String(maximumWidth) &&
        range.step === "0.1" &&
        (await widthMatches(savedWidth))
      );
    },
  );
  await assertUnchanged();
  await clickButton("Back to editor");
  await waitFor("the unopened welcome view with the saved sidebar width", () =>
    welcomeMatches(savedWidth),
  );
  await assertUnchanged();
  console.log(
    resize
      ? "Linux native sidebar: keyboard resized the sidebar and Settings confirmed 48%."
      : "Linux native sidebar: restart applied 48% before opening Settings.",
  );
}
