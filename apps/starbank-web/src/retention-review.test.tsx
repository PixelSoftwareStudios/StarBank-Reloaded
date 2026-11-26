import { renderToStaticMarkup } from "react-dom/server";
import {
  RetentionReviewContents,
  SettingsRetentionReview,
} from "starbank-ui/settings-retention";
import { expect, test } from "vitest";

test("requires a keep count and an open bank for managed backup review", () => {
  const keepAll = renderToStaticMarkup(
    <SettingsRetentionReview keepNewest={null} />,
  );
  expect(keepAll).toContain("Choose how many backups to keep");
  expect(keepAll).toContain('disabled=""');
  const noBank = renderToStaticMarkup(
    <SettingsRetentionReview keepNewest={3} />,
  );
  expect(noBank).toContain("Open a bank to review its managed backups");
  expect(noBank).toContain('disabled=""');
});

test("shows older verified backups and separately reports missing or invalid entries", () => {
  const result = renderToStaticMarkup(
    <RetentionReviewContents
      report={{
        keepNewest: 1,
        verifiedCount: 4,
        retainedCount: 1,
        olderCount: 3,
        missingCount: 1,
        unverifiedCount: 2,
        rejectedMetadata: 3,
        older: [
          {
            name: "Fixture_20261127_091022.sc2bank.bak",
            timestampUtc: "20261127_091022",
            byteLength: 42,
          },
        ],
      }}
    />,
  );
  expect(result).toContain("Read-only review");
  expect(result).toContain("Showing 1 of 3 older backups");
  expect(result).toContain("Fixture_20261127_091022.sc2bank.bak");
  expect(result).toContain("1 missing, 2 unverified, 3 invalid records");
  expect(result).not.toContain("<button");
});
