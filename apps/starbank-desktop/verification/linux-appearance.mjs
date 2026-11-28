import assert from "node:assert/strict";

const settings = 'section[aria-label="Settings"]';
const elementKey = "element-6066-11e4-a52e-4f735466cecf";
const system = ["system", "System"];
const light = ["light", "Light"];
const dark = ["dark", "Dark"];
const colorSchemes = { system: "light dark", light: "light", dark: "dark" };
const compact = ["compact", "Compact"];
const comfortable = ["comfortable", "Comfortable"];

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
      if (!theme || !density) return null;
      if (label !== null) {
        const control = label === "Theme" ? theme : density;
        const options = Array.from(control.options)
          .filter((option) => option.value === choice[0] && option.text === choice[1] && !option.matches(":disabled"));
        return options.length === 1 ? options[0] : null;
      }
      return [theme, density].map((field) => [field.value, field.selectedOptions[0]?.text]);`,
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
  async function applied(theme, density) {
    const appearance = await readApplied();
    return appearance?.theme === theme[0] &&
      appearance.colorScheme === colorSchemes[theme[0]] &&
      appearance.density === density[0] &&
      Number.isFinite(appearance.labelPadding)
      ? appearance
      : null;
  }
  async function confirmedSettings(theme, density) {
    const selected = await readSettings();
    if (JSON.stringify(selected) !== JSON.stringify([theme, density]))
      return null;
    return applied(theme, density);
  }
  async function selectTheme(choice) {
    stage(`Selecting ${choice[1]} on the real Theme control.`);
    await clickChoice("Theme", choice);
    return waitFor(
      `the confirmed ${choice[1]} theme and applied color scheme`,
      () => confirmedSettings(choice, compact),
    );
  }
  async function welcomeWithAppearance(theme, density) {
    if (
      (await content(settings)) !== null ||
      (await content(".editor-welcome h1")) !== "StarBank Reloaded" ||
      (await content('.editor-welcome p[role="status"]')) !== "No bank open."
    )
      return null;
    return applied(theme, density);
  }

  const initialTheme = selectAppearance ? system : dark;
  const initialDensity = selectAppearance ? compact : comfortable;
  const initial = await waitFor(
    "the unopened welcome view with the expected applied appearance",
    () => welcomeWithAppearance(initialTheme, initialDensity),
  );
  const location = new URL(await command("/url"));
  assert.equal(location.protocol, "tauri:");
  assert.equal(location.host, "localhost");
  await assertUnchanged();

  await click('button[aria-label="Settings"]', null);
  await waitFor("the visible enabled Appearance selectors", () =>
    confirmedSettings(initialTheme, initialDensity),
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
      confirmedSettings(dark, comfortable),
    );
    assert(
      updated.labelPadding > initial.labelPadding,
      "Comfortable density must increase the Quick Options label padding.",
    );
  }
  await assertUnchanged();
  await clickButton("Back to editor");
  await waitFor("the welcome view with Dark and Comfortable applied", () =>
    welcomeWithAppearance(dark, comfortable),
  );
  await assertUnchanged();
  stage(
    selectAppearance
      ? "Dark and Comfortable were selected and applied."
      : "Dark and Comfortable were restored before opening Settings.",
  );
}
