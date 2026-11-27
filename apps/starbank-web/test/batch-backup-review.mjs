import assert from "node:assert/strict";

const runId = 37;
const firstReviewRow = 60;
const expiredReviewRow = 61;
const entries = Array.from({ length: 62 }, (_, index) => ({
  bankId: `synthetic-${index}`,
  name: `Fixture-${index + 1}.SC2Bank`,
  outcome: index >= firstReviewRow ? "needs_review" : "created",
  backupName:
    index >= firstReviewRow ? null : `Fixture-${index + 1}.SC2Bank.bak`,
}));
const report = { runId, entries, created: 60, failed: 2, skipped: 0 };
const details = {
  runId,
  rowIndex: firstReviewRow,
  kind: "metadata_commit_uncertain",
  backup: {
    display: "/synthetic/backups/Fixture-61.SC2Bank.bak",
    truncated: false,
    lossy: false,
  },
  recovery: {
    display: "/synthetic/backups/Fixture\uFFFD\r\n.pending",
    truncated: true,
    lossy: true,
  },
  reason: "Synthetic metadata verification failure.",
  reasonTruncated: true,
};

export async function verifyBatchBackupReview(page) {
  await page.evaluate(
    ({ report, details, expiredReviewRow }) => {
      const bridge = window.__TAURI_INTERNALS__;
      const invoke = bridge.invoke;
      const fixture = {
        calls: [],
        holdFirstReply: true,
        release: undefined,
        restore: () => {
          bridge.invoke = invoke;
          delete window.fixtureBatchReview;
        },
      };
      window.fixtureBatchReview = fixture;
      bridge.invoke = async function (command, args) {
        if (command === "create_batch_backup") {
          fixture.calls.push({ command, args });
          return report;
        }
        if (command === "review_batch_backup") {
          fixture.calls.push({ command, args });
          if (args.target?.rowIndex === expiredReviewRow)
            throw "invalid_review";
          if (fixture.holdFirstReply) {
            fixture.holdFirstReply = false;
            return new Promise((resolve) => {
              fixture.release = () => resolve(details);
            });
          }
          return details;
        }
        return invoke.call(this, command, args);
      };
    },
    { report, details, expiredReviewRow },
  );

  try {
    await page.getByRole("button", { name: "Tools", exact: true }).click();
    await page
      .getByRole("menuitem", { name: "Back up all bank files", exact: true })
      .click();
    await page.getByRole("button", { name: "Back up", exact: true }).click();
    const dialog = page.getByRole("dialog", {
      name: "Backup results",
      exact: true,
    });
    const table = dialog.getByRole("table", { name: "Bank backup results" });
    await table.waitFor();
    assert.equal(
      await table.getByRole("button", { name: /^Review / }).count(),
      0,
    );
    await dialog.getByRole("button", { name: "Next", exact: true }).click();
    const pageNumber = dialog.getByText("61–62 of 62", { exact: true });
    await pageNumber.waitFor();
    assert.equal(
      await table.getByRole("button", { name: /^Review / }).count(),
      2,
    );

    const review = table.getByRole("button", {
      name: `Review ${entries[firstReviewRow].name}`,
      exact: true,
    });
    const expired = table.getByRole("button", {
      name: `Review ${entries[expiredReviewRow].name}`,
      exact: true,
    });
    const back = dialog.getByRole("button", { name: "Back", exact: true });
    const backup = dialog.getByRole("textbox", {
      name: "Recorded backup location",
      exact: true,
    });
    const recovery = dialog.getByRole("textbox", {
      name: "Recorded recovery location",
      exact: true,
    });

    await review.click();
    await dialog
      .getByRole("status")
      .filter({ hasText: "Reading backup review…" })
      .waitFor();
    await page.waitForFunction(
      () => typeof window.fixtureBatchReview.release === "function",
    );
    await back.click();
    await pageNumber.waitFor();
    await review.and(page.locator(":focus")).waitFor();

    await expired.click();
    const unavailable = dialog.getByRole("alert").filter({
      hasText: "That backup review is no longer available.",
    });
    await unavailable.waitFor();
    await page.evaluate(async () => {
      window.fixtureBatchReview.release();
      // Allow the obsolete response and the renderer's resulting work to settle.
      await new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      );
    });
    assert.equal(await unavailable.isVisible(), true);
    assert.equal(await backup.count(), 0);
    assert.equal(await recovery.count(), 0);
    await back.click();
    await pageNumber.waitFor();
    await expired.and(page.locator(":focus")).waitFor();

    await review.click();
    await dialog
      .getByText(/could not verify the backup metadata write/i)
      .waitFor();
    await dialog.getByText(/their current status is unknown/i).waitFor();
    assert.equal(await backup.inputValue(), details.backup.display);
    assert.equal(await backup.evaluate((field) => field.readOnly), true);
    assert.equal(await recovery.evaluate((field) => field.readOnly), true);
    assert.deepEqual(
      await recovery.evaluate((field) => {
        field.select();
        const clipboardData = new DataTransfer();
        const event = new ClipboardEvent("copy", {
          bubbles: true,
          cancelable: true,
          clipboardData,
        });
        field.dispatchEvent(event);
        return {
          prevented: event.defaultPrevented,
          text: clipboardData.getData("text/plain"),
        };
      }),
      { prevented: true, text: details.recovery.display },
    );
    await dialog
      .getByText("The recorded path was shortened.", { exact: true })
      .waitFor();
    await dialog
      .getByText("Some path characters could not be represented.", {
        exact: true,
      })
      .waitFor();
    await dialog
      .getByText("The reported reason was shortened.", { exact: true })
      .waitFor();
    await dialog.getByText(details.reason, { exact: true }).waitFor();

    await back.click();
    await pageNumber.waitFor();
    await review.and(page.locator(":focus")).waitFor();
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    await dialog.waitFor({ state: "hidden" });
    assert.deepEqual(
      await page.evaluate(() => window.fixtureBatchReview.calls),
      [
        {
          command: "create_batch_backup",
          args: { scope: { kind: "all" }, placement: "managed" },
        },
        ...[firstReviewRow, expiredReviewRow, firstReviewRow].map(
          (rowIndex) => ({
            command: "review_batch_backup",
            args: { target: { runId, rowIndex } },
          }),
        ),
      ],
    );
  } finally {
    await page.evaluate(() => window.fixtureBatchReview?.restore());
  }
}
