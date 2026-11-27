import assert from "node:assert/strict";

const large = "9007199254740993";
const fixed = "-0012345678.0900";
const fixtures =
  '<Bank><Section name="Player"><Key name="Large"><Value int="' +
  large +
  '"/></Key><Key name="Fixed"><Value fixed="' +
  fixed +
  '"/></Key></Section></Bank>';

async function exactValue(page, field, value) {
  await page.waitForFunction(({ input, value }) => input.value === value, {
    input: await field.elementHandle(),
    value,
  });
}

async function settleFocus(page) {
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
}

async function pointerGesture(page, points) {
  if (points.length === 1) {
    await page.mouse.click(points[0].x, points[0].y);
    return;
  }
  const [start, end] = points;
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(end.x, end.y, { steps: 5 });
  await page.mouse.up();
}

async function referenceSelection(page, field, offsets) {
  const points = await field.evaluate((input, offsets) => {
    const rect = input.getBoundingClientRect();
    const css = getComputedStyle(input);
    const context = document.createElement("canvas").getContext("2d");
    context.font = [
      css.fontStyle,
      css.fontWeight,
      css.fontSize,
      css.fontFamily,
    ].join(" ");
    const reference = input.cloneNode(false);
    reference.id = "numeric-caret-reference";
    reference.removeAttribute("aria-label");
    reference.value = input.value;
    Object.assign(reference.style, {
      position: "fixed",
      left: `${rect.left}px`,
      top: `${rect.top}px`,
      width: `${rect.width}px`,
      height: `${rect.height}px`,
      margin: "0",
      zIndex: "10000",
    });
    input.parentElement.append(reference);
    const left =
      rect.left +
      Number.parseFloat(css.borderLeftWidth) +
      Number.parseFloat(css.paddingLeft);
    return offsets.map((offset) => ({
      x: left + context.measureText(input.value.slice(0, offset)).width,
      y: rect.top + rect.height / 2,
    }));
  }, offsets);

  const reference = page.locator("#numeric-caret-reference");
  await pointerGesture(page, points);
  const selection = await reference.evaluate((input) => {
    const start = input.value.slice(0, input.selectionStart);
    const end = input.value.slice(0, input.selectionEnd);
    return {
      start: start.replaceAll(",", "").length,
      end: end.replaceAll(",", "").length,
      direction: input.selectionDirection,
    };
  });
  await reference.evaluate((input) => input.remove());
  return { points, selection };
}

async function assertSelection(field, expected) {
  assert.deepEqual(
    await field.evaluate((input) => ({
      start: input.selectionStart,
      end: input.selectionEnd,
      direction: input.selectionDirection,
    })),
    expected,
  );
}

export async function verifyNumericCaret(browser, url, bankPicker) {
  const context = await browser.newContext({
    viewport: { width: 850, height: 500 },
  });
  try {
    const page = await context.newPage();
    const errors = [];
    page.setDefaultTimeout(10_000);
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(url);
    await bankPicker(page)("NumericCaret.SC2Bank", fixtures);
    const largeField = page.getByRole("textbox", {
      name: "Large · Value value",
      exact: true,
    });
    const fixedField = page.getByRole("textbox", {
      name: "Fixed · Value value",
      exact: true,
    });
    const tableTab = page.getByRole("tab", { name: "Table View", exact: true });
    const cases = [
      {
        field: largeField,
        value: large,
        offsets: [[3], [17], [5, 18], [18, 7]],
      },
      {
        field: fixedField,
        value: fixed,
        offsets: [[2], [16], [2, 17], [19, 10]],
      },
    ];

    for (const { field, value, offsets } of cases) {
      for (const gesture of offsets) {
        await tableTab.focus();
        const grouped = await field.inputValue();
        assert(grouped.includes(","));
        assert.equal(grouped.replaceAll(",", ""), value);
        const expected = await referenceSelection(page, field, gesture);
        assert(expected.selection.start < value.length);
        if (gesture.length > 1)
          assert(expected.selection.end > expected.selection.start);
        await pointerGesture(page, expected.points);
        await exactValue(page, field, value);
        await assertSelection(field, expected.selection);
      }
    }

    await tableTab.focus();
    const early = await referenceSelection(page, largeField, [17]);
    const { x, y } = early.points[0];
    await page.mouse.move(x, y);
    await page.mouse.down();
    const expectedEdit =
      large.slice(0, early.selection.start) +
      "7" +
      large.slice(early.selection.end);
    await page.keyboard.press("7");
    await page.mouse.up();
    await exactValue(page, largeField, expectedEdit);
    assert.equal(await largeField.getAttribute("data-table-draft"), "true");
    await page
      .getByRole("button", { name: "Discard draft", exact: true })
      .click();

    const grouped = await fixedField.inputValue();
    const composition = await referenceSelection(page, fixedField, [16]);
    const point = composition.points[0];
    await page.mouse.move(point.x, point.y);
    await page.mouse.down();
    await fixedField.evaluate((input) => {
      input.dispatchEvent(
        new CompositionEvent("compositionstart", { bubbles: true }),
      );
    });
    await page.mouse.up();
    await settleFocus(page);
    assert.equal(await fixedField.inputValue(), grouped);
    await fixedField.evaluate((input) => {
      input.dispatchEvent(
        new CompositionEvent("compositionend", { bubbles: true }),
      );
    });
    await exactValue(page, fixedField, fixed);

    await tableTab.focus();
    await page.mouse.move(point.x, point.y);
    await page.mouse.down();
    await tableTab.focus();
    await page.mouse.up();
    await settleFocus(page);
    assert.equal(await fixedField.inputValue(), grouped);
    assert.equal(await tableTab.and(page.locator(":focus")).count(), 1);

    await page.mouse.move(point.x, point.y);
    await page.mouse.down();
    await fixedField.evaluate((input) => input.select());
    await page.keyboard.insertText("1,,2");
    await page.mouse.up();
    await fixedField.and(page.locator('[aria-invalid="true"]')).waitFor();
    assert.equal(await fixedField.inputValue(), "1,,2");
    assert.deepEqual(errors, []);
  } finally {
    await context.close();
  }
}
