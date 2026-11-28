import assert from "node:assert/strict";

const options = [
  { key: "showBreadcrumbs", label: "Show path breadcrumbs", initial: true },
  { key: "wrapRaw", label: "Wrap Raw View lines", initial: false },
  { key: "groupNumbers", label: "Group numbers with commas", initial: true },
];

export const presentationSettingKeys = options.map(({ key }) => key);

export async function verifyPresentationSettings(page, bridge) {
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = page.getByRole("region", { name: "Settings", exact: true });
  const control = ({ label }, checked) =>
    settings.getByRole("checkbox", { name: label, exact: true, checked });
  const replies = () => page.evaluate(() => window.fixturePresentationReplies);
  const waitForRequests = (count) =>
    page.waitForFunction(
      (value) => window.fixturePresentationRequests === value,
      count,
    );
  const waitForReplies = (count) =>
    page.waitForFunction(
      (value) => window.fixturePresentationReplies === value,
      count,
    );
  const change = ({ key }, value) => ({ change: { key, value } });

  for (const option of options) {
    const checkbox = control(option);
    const before = await replies();
    assert.equal(await checkbox.isChecked(), option.initial);
    const releaseFirst = bridge.holdNextPresentationWrite();
    const releaseSecond = bridge.holdNextPresentationWrite();
    try {
      await checkbox.click();
      await waitForRequests(before + 1);
      assert.equal(await checkbox.isChecked(), !option.initial, option.label);
      await checkbox.click();
      assert.equal(await replies(), before);
      assert.equal(await checkbox.isChecked(), option.initial, option.label);

      releaseFirst();
      await waitForRequests(before + 2);
      assert.equal(await replies(), before + 1);
      assert.equal(await checkbox.isChecked(), option.initial, option.label);
    } finally {
      releaseFirst();
      releaseSecond();
    }
    await waitForReplies(before + 2);
    assert.deepEqual(bridge.callsFor("change_setting").slice(before), [
      change(option, !option.initial),
      change(option, option.initial),
    ]);
    assert.equal(await checkbox.isChecked(), option.initial, option.label);
  }

  const [, wrap, numbers] = options;
  const before = await replies();
  const readCount = bridge.callsFor("read_settings").length;
  const reject = bridge.holdNextPresentationWrite();
  try {
    await control(wrap).click();
    await waitForRequests(before + 1);
    assert.equal(await control(wrap).isChecked(), true);
    assert.equal(await control(numbers).isChecked(), true);
    bridge.setStoredPresentationSetting(numbers.key, false);
  } finally {
    reject("settings_changed");
  }
  await waitForReplies(before + 1);
  await settings
    .getByRole("alert")
    .filter({
      hasText: "The saved settings changed. Review them and try again.",
    })
    .waitFor();
  await control(wrap, false).waitFor();
  await control(numbers, false).waitFor();
  assert.equal(bridge.callsFor("read_settings").length, readCount + 1);
  assert.deepEqual(bridge.callsFor("change_setting").slice(before), [
    change(wrap, true),
  ]);
}
