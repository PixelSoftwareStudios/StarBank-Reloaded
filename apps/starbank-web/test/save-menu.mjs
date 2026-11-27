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
  const triggerBounds = await trigger.boundingBox();
  const menuBounds = await menu.boundingBox();
  assert(triggerBounds && menuBounds);
  assert(
    menuBounds.y + menuBounds.height <= triggerBounds.y + 1,
    "The save menu should open above its trigger",
  );
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });
  await page.waitForFunction(() =>
    document.activeElement?.matches('button[aria-label="More save options"]'),
  );
  await page.locator(".document-editor h1").evaluate((heading) => {
    heading.setAttribute("tabindex", "-1");
    heading.focus();
    heading.removeAttribute("tabindex");
  });
  await trigger.focus();
  await help.waitFor();

  await trigger.click();
  await menu.waitFor();
  await page.locator(".document-editor h1").click();
  await menu.waitFor({ state: "hidden" });

  await trigger.press("ArrowDown");
  await copy.waitFor();
  await page.keyboard.press("Enter");
  await menu.waitFor({ state: "hidden" });
}
