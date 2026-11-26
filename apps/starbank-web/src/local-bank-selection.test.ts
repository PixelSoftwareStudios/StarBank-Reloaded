import { describe, expect, test, vi } from "vitest";
import {
  collectLocalBanks,
  MAX_LOCAL_BANKS,
  MAX_SELECTED_FILES,
} from "./local-bank-selection";

const BYTE_LIMIT = 8;

function file(name: string, contents: string, relativePath?: string) {
  const candidate = new File([contents], name);
  if (relativePath) {
    Object.defineProperty(candidate, "webkitRelativePath", {
      value: relativePath,
    });
  }
  return candidate;
}

describe("local bank selection", () => {
  test("uses browser-provided relative paths and ignores unrelated folder entries", () => {
    const first = file("Bank.SC2BANK", "first", "Banks/One/Bank.SC2BANK");
    const second = file("Bank.SC2BANK", "second", "Banks/Two/Bank.SC2BANK");
    const readFirst = vi.spyOn(first, "arrayBuffer");
    const readSecond = vi.spyOn(second, "arrayBuffer");
    const result = collectLocalBanks(
      [
        first,
        file("settings.xml", "unrelated"),
        file("Bank.SC2Bank", "oversized"),
        second,
      ],
      "folder",
      BYTE_LIMIT,
    );

    expect(result).toEqual({
      banks: [
        { file: first, label: "Banks/One/Bank.SC2BANK" },
        { file: second, label: "Banks/Two/Bank.SC2BANK" },
      ],
      ignored: 1,
      oversized: 1,
      truncated: false,
    });
    expect(readFirst).not.toHaveBeenCalled();
    expect(readSecond).not.toHaveBeenCalled();
  });

  test("accepts explicitly selected XML and an exact size limit", () => {
    const exactSize = file("Archive.XML", "12345678");
    const result = collectLocalBanks(
      [
        exactSize,
        file("Backup.sc2bank.bak", "older"),
        file("Archive.sc2bank", "123456789"),
      ],
      "files",
      BYTE_LIMIT,
    );

    expect(result.banks).toEqual([{ file: exactSize, label: "Archive.XML" }]);
    expect(result).toMatchObject({
      ignored: 1,
      oversized: 1,
      truncated: false,
    });
  });

  test("bounds retained banks and the number of unrelated entries inspected", () => {
    const bank = file("Bank.sc2bank", "payload");
    const invalid = file("Screenshot.png", "ignored");
    const manyBanks = Array.from({ length: MAX_LOCAL_BANKS + 1 }, () => bank);
    const manyFiles = Array.from(
      { length: MAX_SELECTED_FILES + 1 },
      () => invalid,
    );
    manyFiles[MAX_SELECTED_FILES] = bank;

    const retained = collectLocalBanks(manyBanks, "files", BYTE_LIMIT);
    expect(retained.banks).toHaveLength(MAX_LOCAL_BANKS);
    expect(retained.truncated).toBe(true);
    const inspected = collectLocalBanks(manyFiles, "folder", BYTE_LIMIT);
    expect(inspected.banks).toHaveLength(0);
    expect(inspected).toMatchObject({
      ignored: MAX_SELECTED_FILES,
      truncated: true,
    });
  });
});
