import {
  type RetentionReview,
  SettingsRetentionReview,
} from "starbank-ui/settings-retention";
import { type ActiveBankTarget, readBackupRetentionReview } from "./transport";

const REVIEW_ERRORS = {
  keep_all: "The retention setting is Keep all.",
  review_too_large:
    "This bank’s managed backup history exceeds the review limit.",
  same_folder:
    "Choose a separate managed folder to review older backups for this bank.",
  scope_changed:
    "The managed folder or retention setting changed. Review it again.",
  stale_bank: "The open bank changed. Review it again.",
  unavailable: "The managed backup review is unavailable.",
} as const;

async function reviewBackupRetention(
  target: ActiveBankTarget,
): Promise<RetentionReview> {
  try {
    return await readBackupRetentionReview(target);
  } catch (error) {
    const reason =
      typeof error === "string" && Object.hasOwn(REVIEW_ERRORS, error)
        ? REVIEW_ERRORS[error as keyof typeof REVIEW_ERRORS]
        : REVIEW_ERRORS.unavailable;
    throw new Error(reason);
  }
}

export function BackupRetentionReview({
  target,
  keepNewest,
  disabled,
}: {
  readonly target: ActiveBankTarget | undefined;
  readonly keepNewest: number | null;
  readonly disabled?: boolean;
}) {
  return (
    <SettingsRetentionReview
      key={JSON.stringify([target?.documentId, target?.revision, keepNewest])}
      keepNewest={keepNewest}
      disabled={disabled}
      onReview={target ? () => reviewBackupRetention(target) : undefined}
    />
  );
}
