import assert from "node:assert/strict";

export async function verifySaveMenu(page) {
  const trigger = page.getByRole("button", {
    name: "More save options",
    exact: true,
  });
  const menu = page.getByRole("menu", {
    name: "More save options",
    exact: true,
  });
  const copy = menu.getByRole("menuitem", {
    name: "Copy bank XML to clipboard",
    exact: true,
  });
  const help = page.getByRole("tooltip").filter({
    hasText: "Choose how to save or copy the open bank",
  });

  await page.mouse.move(1, 1);
  await trigger.hover();
  await help.waitFor();
  await page.mouse.move(0, 0);
  await help.waitFor({ state: "hidden" });

  await trigger.press("ArrowDown");
  await copy.waitFor();
  assert.equal(await menu.getByRole("menuitem").count(), 1);
  const popover = menu.locator("..");
  assert.equal(await popover.getAttribute("data-placement"), "top");
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });
  assert.equal(
    await trigger.evaluate((button) => document.activeElement === button),
    true,
  );
  await trigger.press("Shift+Tab");
  await page.keyboard.press("Tab");
  await help.waitFor();

  await trigger.click();
  await menu.waitFor();
  const title = await page.locator(".document-editor h1").boundingBox();
  assert(title, "The rectangle outside the save menu must be visible");
  await page.mouse.click(title.x + title.width / 2, title.y + title.height / 2);
  await menu.waitFor({ state: "hidden" });

  await trigger.press("ArrowDown");
  await copy.waitFor();
  await page.keyboard.press("Enter");
  await menu.waitFor({ state: "hidden" });
}
