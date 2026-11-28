import assert from "node:assert/strict";

const settings = 'section[aria-label="Settings"]';
const elementKey = "element-6066-11e4-a52e-4f735466cecf";
const system = ["system", "System"];
const light = ["light", "Light"];
const dark = ["dark", "Dark"];
const colorSchemes = { system: "light dark", light: "light", dark: "dark" };
const compact = ["compact", "Compact"];
const comfortable = ["comfortable", "Comfortable"];
const small = ["small", "Small"];
const normal = ["normal", "Normal"];
const large = ["large", "Large"];
const rootFontPx = { small: 14, normal: 16, large: 18 };
const optionLabelRem = 0.8125;

function stage(message) {
  console.log(`Linux native appearance: ${message}`);
}

export async function verifyAppearancePreferences({
  command,
  waitFor,
  controls,
  selectAppearance,
  assertUnchanged,
}) {
  assert.equal(typeof selectAppearance, "boolean");
  const { click, clickButton, content, evaluate } = controls;

  const readApplied = () =>
    evaluate(`
      const shells = document.querySelectorAll(".starbank-shell");
      if (shells.length !== 1) return null;
      const label = shells[0].querySelector(".map-options fieldset > label");
      if (!label || label.getClientRects().length === 0) return null;
      const html = document.documentElement;
      return {
        theme: html.dataset.starbankTheme,
        colorScheme: getComputedStyle(html).colorScheme,
        background: getComputedStyle(shells[0]).backgroundColor,
        prefersDark: matchMedia("(prefers-color-scheme: dark)").matches,
        density: shells[0].dataset.density,
        textSize: html.dataset.starbankTextSize,
        rootFontPx: Number.parseFloat(getComputedStyle(html).fontSize),
        labelText: label.textContent.trim(),
        labelFontPx: Number.parseFloat(getComputedStyle(label).fontSize),
        labelPadding: Number.parseFloat(getComputedStyle(label).paddingBlockStart),
      };`);
  const readSettings = (label = null, choice = null) =>
    evaluate(
      `const [selector, label, choice] = arguments;
      const panels = document.querySelectorAll(selector);
      if (panels.length !== 1 || panels[0].querySelector('[role="alert"]')) return null;
      const groups = Array.from(panels[0].querySelectorAll("fieldset"))
        .filter((group) => group.querySelector(":scope > legend")?.textContent.trim() === "Appearance");
      if (groups.length !== 1) return null;
      function labelledSelect(name) {
        const fields = Array.from(groups[0].querySelectorAll("select"))
          .filter((field) => field.labels?.[0]?.querySelector(":scope > span")?.textContent.trim() === name);
        if (fields.length !== 1 || fields[0].matches(":disabled") || fields[0].getClientRects().length === 0) return null;
        return fields[0];
      }
      const theme = labelledSelect("Theme");
      const density = labelledSelect("Density");
      const textSize = labelledSelect("Text size");
      if (!theme || !density || !textSize) return null;
      if (label !== null) {
        const control = { Theme: theme, Density: density, "Text size": textSize }[label];
        if (!control) return null;
        const options = Array.from(control.options)
          .filter((option) => option.value === choice[0] && option.text === choice[1] && !option.matches(":disabled"));
        return options.length === 1 ? options[0] : null;
      }
      return [theme, density, textSize].map((field) => [field.value, field.selectedOptions[0]?.text]);`,
      settings,
      label,
      choice,
    );
  async function clickChoice(label, choice) {
    const option = await waitFor(
      `the enabled ${label} option ${choice[1]}`,
      () => readSettings(label, choice),
    );
    assert.equal(typeof option?.[elementKey], "string", "Missing option ID.");
    await command(`/element/${option[elementKey]}/click`, "POST", {});
  }
  async function applied(theme, density, textSize) {
    const appearance = await readApplied();
    return appearance?.theme === theme[0] &&
      appearance.colorScheme === colorSchemes[theme[0]] &&
      appearance.density === density[0] &&
      appearance.textSize === textSize[0] &&
      appearance.rootFontPx === rootFontPx[textSize[0]] &&
      appearance.labelText === "Hide Blizzard maps" &&
      appearance.labelFontPx === rootFontPx[textSize[0]] * optionLabelRem &&
      Number.isFinite(appearance.labelPadding)
      ? appearance
      : null;
  }
  async function confirmedSettings(theme, density, textSize) {
    const selected = await readSettings();
    if (JSON.stringify(selected) !== JSON.stringify([theme, density, textSize]))
      return null;
    return applied(theme, density, textSize);
  }
  async function selectTheme(choice) {
    stage(`Selecting ${choice[1]} on the real Theme control.`);
    await clickChoice("Theme", choice);
    return waitFor(
      `the confirmed ${choice[1]} theme and applied color scheme`,
      () => confirmedSettings(choice, compact, normal),
    );
  }
  async function welcomeWithAppearance(theme, density, textSize) {
    if (
      (await content(settings)) !== null ||
      (await content(".editor-welcome h1")) !== "StarBank Reloaded" ||
      (await content('.editor-welcome p[role="status"]')) !== "No bank open."
    )
      return null;
    return applied(theme, density, textSize);
  }

  const initialTheme = selectAppearance ? system : dark;
  const initialDensity = selectAppearance ? compact : comfortable;
  const initialTextSize = selectAppearance ? normal : large;
  const initial = await waitFor(
    "the unopened welcome view with the expected applied appearance",
    () => welcomeWithAppearance(initialTheme, initialDensity, initialTextSize),
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await assertUnchanged();

  await click('button[aria-label="Settings"]', null);
  await waitFor("the visible enabled Appearance selectors", () =>
    confirmedSettings(initialTheme, initialDensity, initialTextSize),
  );
  if (selectAppearance) {
    const lightAppearance = await selectTheme(light);
    const systemAppearance = await selectTheme(system);
    const darkAppearance = await selectTheme(dark);
    assert.notEqual(
      lightAppearance.background,
      darkAppearance.background,
      "Explicit Light and Dark must use different shell backgrounds.",
    );
    const preferredAppearance = systemAppearance.prefersDark
      ? darkAppearance
      : lightAppearance;
    assert.equal(
      systemAppearance.background,
      preferredAppearance.background,
      "The System shell background must match WebKit's preferred scheme.",
    );
    stage(
      `System matched the ${preferredAppearance.theme} preference reported by WebKit.`,
    );

    stage("Selecting Comfortable on the real Density control.");
    await clickChoice("Density", comfortable);
    const updated = await waitFor("the confirmed Comfortable density", () =>
      confirmedSettings(dark, comfortable, normal),
    );
    assert(
      updated.labelPadding > initial.labelPadding,
      "Comfortable density must increase the Quick Options label padding.",
    );
    let previousFont = initial.labelFontPx;
    for (const choice of [small, large]) {
      stage(`Selecting ${choice[1]} on the real Text size control.`);
      await clickChoice("Text size", choice);
      const typography = await waitFor(
        `the confirmed ${choice[1]} text size and applied typography`,
        () => confirmedSettings(dark, comfortable, choice),
      );
      assert(
        choice === small
          ? typography.labelFontPx < previousFont
          : typography.labelFontPx > previousFont,
        "Quick Options text must shrink for Small and grow for Large.",
      );
      previousFont = typography.labelFontPx;
    }
  }
  await assertUnchanged();
  await clickButton("Back to editor");
  await waitFor(
    "the welcome view with Dark, Comfortable and Large applied",
    () => welcomeWithAppearance(dark, comfortable, large),
  );
  await assertUnchanged();
  stage(
    selectAppearance
      ? "Dark, Comfortable and Large were selected and applied."
      : "Dark, Comfortable and Large were restored before opening Settings.",
  );
}
