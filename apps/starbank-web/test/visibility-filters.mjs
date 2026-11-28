import assert from "node:assert/strict";

const filters = [
  {
    key: "hideBlizzardMaps",
    label: "Hide Blizzard maps",
    mapName: "Synthetic Blizzard Map",
  },
  {
    key: "hideMapsWithoutBanks",
    label: "Hide maps without bank files",
    mapName: "Synthetic Empty Map",
  },
];

export async function verifyVisibilityFilters(page, bridge) {
  const quick = page.getByRole("group", { name: "Quick Options", exact: true });
  const settings = page.getByRole("region", { name: "Settings", exact: true });
  const maps = page.getByRole("list", { name: "Maps", exact: true });
  const waitForRequests = (count) =>
    page.waitForFunction(
      (value) => window.fixtureFilterRequests === value,
      count,
    );
  const waitForReplies = (count) =>
    page.waitForFunction(
      (value) => window.fixtureFilterReplies === value,
      count,
    );
  const replies = () => page.evaluate(() => window.fixtureFilterReplies);
  const checkbox = (surface, filter) =>
    surface.getByRole("checkbox", { name: filter.label, exact: true });
  const mapRow = (filter) =>
    maps.getByRole("button", { name: filter.mapName, exact: true });
  const change = (filter, value) => ({ change: { key: filter.key, value } });

  async function checkFilter(filter, hidden) {
    assert.deepEqual(
      {
        quickChecked: await checkbox(quick, filter).isChecked(),
        settingsChecked: await checkbox(settings, filter).isChecked(),
        mapVisible: await mapRow(filter).isVisible(),
      },
      { quickChecked: hidden, settingsChecked: hidden, mapVisible: !hidden },
      filter.label,
    );
  }

  async function verifyReversal(filter, firstSurface, secondSurface) {
    const before = await replies();
    const releaseFirst = bridge.holdNextFilterWrite();
    const releaseSecond = bridge.holdNextFilterWrite();
    try {
      await checkbox(firstSurface, filter).click();
      await waitForRequests(before + 1);
      await checkFilter(filter, false);
      await checkbox(secondSurface, filter).click();
      assert.equal(await replies(), before);
      await checkFilter(filter, true);

      releaseFirst();
      await waitForRequests(before + 2);
      assert.equal(await replies(), before + 1);
      await checkFilter(filter, true);
    } finally {
      releaseFirst();
      releaseSecond();
    }
    await waitForReplies(before + 2);
    assert.deepEqual(bridge.callsFor("change_setting").slice(before), [
      change(filter, false),
      change(filter, true),
    ]);
    await checkFilter(filter, true);
  }

  async function verifyIsolatedFailure() {
    const [official, empty] = filters;
    const before = await replies();
    const reads = bridge.callsFor("read_settings").length;
    const releaseFirst = bridge.holdNextFilterWrite();
    const rejectSecond = bridge.holdNextFilterWrite();
    try {
      await checkbox(quick, official).click();
      await waitForRequests(before + 1);
      await checkFilter(official, false);
      await checkFilter(empty, true);
      await checkbox(settings, empty).click();
      assert.equal(await replies(), before);
      await checkFilter(official, false);
      await checkFilter(empty, false);

      releaseFirst();
      await waitForRequests(before + 2);
      assert.equal(await replies(), before + 1);
      await checkFilter(official, false);
      await checkFilter(empty, false);
      bridge.setStoredFilter(official.key, true);
    } finally {
      releaseFirst();
      rejectSecond("settings_changed");
    }

    await waitForReplies(before + 2);
    await settings
      .getByRole("alert")
      .filter({
        hasText: "The saved settings changed. Review them and try again.",
      })
      .waitFor();
    await mapRow(official).waitFor({ state: "hidden" });
    await mapRow(empty).waitFor({ state: "hidden" });
    await checkFilter(official, true);
    await checkFilter(empty, true);
    assert.equal(bridge.callsFor("read_settings").length, reads + 1);
    assert.deepEqual(bridge.callsFor("change_setting").slice(before), [
      change(official, false),
      change(empty, false),
    ]);
  }

  await maps.getByRole("button", { name: "Synthetic Map" }).waitFor();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await settings.waitFor();
  for (const filter of filters) await checkFilter(filter, true);
  await verifyReversal(filters[0], quick, settings);
  await verifyReversal(filters[1], settings, quick);
  await verifyIsolatedFailure();
}
