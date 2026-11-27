import assert from "node:assert/strict";

export async function verifySidebarResize(page, bridge) {
  const splitter = page.getByRole("separator", { name: "Resize map list" });
  const shell = page.locator(".starbank-shell");
  const waitForRequests = (count) =>
    page.waitForFunction(
      (count) => window.fixtureWidthRequests === count,
      count,
    );
  const waitForReplies = (count) =>
    page.waitForFunction(
      (count) => window.fixtureWidthReplies === count,
      count,
    );
  async function checkWidth(width) {
    assert.equal(await splitter.getAttribute("aria-valuenow"), `${width}`);
    assert.equal(
      await shell.evaluate((element) =>
        element.style.getPropertyValue("--map-pane-width"),
      ),
      `${width}%`,
    );
  }
  const waitForWidth = (width) =>
    splitter.and(page.locator(`[aria-valuenow="${width}"]`)).waitFor();

  await splitter.and(page.locator(':not([aria-disabled="true"])')).waitFor();
  await checkWidth(28);
  const first = bridge.holdNextWidthWrite();
  const second = bridge.holdNextWidthWrite();
  await splitter.press("ArrowRight");
  await waitForRequests(1);
  await checkWidth(30);
  await splitter.press("ArrowRight");
  await splitter.press("ArrowRight");
  await checkWidth(34);
  assert.equal(bridge.callsFor("change_setting").length, 1);
  first();
  await waitForRequests(2);
  await waitForReplies(1);
  await checkWidth(34);
  second();
  await waitForReplies(3);
  assert.deepEqual(
    bridge.callsFor("change_setting").map(({ change }) => change.value),
    [30, 32, 34],
  );
  await checkWidth(34);

  const bounds = await splitter.boundingBox();
  assert(bounds);
  const x = bounds.x + bounds.width / 2;
  const y = bounds.y + bounds.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 30, y);
  const dragged = await splitter.getAttribute("aria-valuenow");
  assert(Number(dragged) > 34);
  const releaseDrag = bridge.holdNextWidthWrite();
  await page.mouse.up();
  await waitForRequests(4);
  await checkWidth(dragged);
  assert.equal(
    bridge.callsFor("change_setting").at(-1).change.value,
    Number(dragged),
  );
  releaseDrag();
  await waitForReplies(4);

  // A fresh native snapshot can carry widths confirmed outside this editor.
  bridge.setStoredWidth(42);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const settings = page.getByRole("region", { name: "Settings" });
  await settings.getByLabel("Map index workers").selectOption("2");
  await waitForWidth(42);
  await checkWidth(42);
  await settings.getByRole("button", { name: "Back to editor" }).click();

  const reject = bridge.holdNextWidthWrite();
  await splitter.press("ArrowLeft");
  await waitForRequests(5);
  await checkWidth(40);
  const reads = bridge.callsFor("read_settings").length;
  bridge.setStoredWidth(39);
  reject("settings_changed");
  await waitForWidth(39);
  await checkWidth(39);
  assert.equal(bridge.callsFor("read_settings").length, reads + 1);
}
