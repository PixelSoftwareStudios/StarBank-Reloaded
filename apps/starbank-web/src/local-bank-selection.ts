export const MAX_SELECTED_FILES = 4096;
export const MAX_LOCAL_BANKS = 512;

export type BankSelectionSource = "files" | "folder";

export interface LocalBank {
  readonly file: File;
  readonly label: string;
}

export interface LocalBankSelection {
  readonly banks: readonly LocalBank[];
  readonly ignored: number;
  readonly oversized: number;
  readonly truncated: boolean;
}

export function collectLocalBanks(
  files: ArrayLike<File>,
  source: BankSelectionSource,
  maxBankBytes: number,
): LocalBankSelection {
  const banks: LocalBank[] = [];
  let ignored = 0;
  let oversized = 0;
  const inspectionLimit = Math.min(files.length, MAX_SELECTED_FILES);
  let inspected = 0;

  while (inspected < inspectionLimit && banks.length < MAX_LOCAL_BANKS) {
    const file = files[inspected];
    inspected += 1;
    const name = file.name.toLowerCase();
    if (
      !name.endsWith(".sc2bank") &&
      !(source === "files" && name.endsWith(".xml"))
    ) {
      ignored += 1;
    } else if (file.size > maxBankBytes) {
      oversized += 1;
    } else {
      banks.push({
        file,
        label:
          source === "folder" && file.webkitRelativePath
            ? file.webkitRelativePath
            : file.name,
      });
    }
  }

  return { banks, ignored, oversized, truncated: inspected < files.length };
}
