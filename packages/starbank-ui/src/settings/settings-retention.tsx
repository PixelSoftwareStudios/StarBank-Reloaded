import { useId, useRef, useState } from "react";
import { EditorDialog } from "../editor/editor-dialog";
import { settingsButtonClasses } from "./settings-controls";

interface OlderBackup {
  readonly name: string;
  readonly timestampUtc: string;
  readonly byteLength: number;
}

export interface RetentionReview {
  readonly keepNewest: number;
  readonly verifiedCount: number;
  readonly retainedCount: number;
  readonly olderCount: number;
  readonly missingCount: number;
  readonly unverifiedCount: number;
  readonly rejectedMetadata: number;
  readonly older: readonly OlderBackup[];
}

type ReviewState =
  | { readonly kind: "idle" }
  | { readonly kind: "running" }
  | { readonly kind: "result"; readonly report: RetentionReview }
  | { readonly kind: "error"; readonly message: string };

interface SettingsRetentionReviewProps {
  readonly keepNewest: number | null;
  readonly disabled?: boolean;
  readonly onReview?: () => Promise<RetentionReview>;
}

export function SettingsRetentionReview({
  keepNewest,
  disabled = false,
  onReview,
}: SettingsRetentionReviewProps) {
  const noteId = useId();
  const running = useRef(false);
  const [state, setState] = useState<ReviewState>({ kind: "idle" });
  const disabledReason =
    keepNewest === null
      ? "Choose how many backups to keep to review older copies."
      : !onReview
        ? "Open a bank to review its managed backups."
        : undefined;

  async function review() {
    if (!onReview || keepNewest === null || disabled || running.current) return;
    running.current = true;
    setState({ kind: "running" });
    try {
      const report = await onReview();
      if (report.keepNewest !== keepNewest) {
        throw new Error("The retention setting changed. Review it again.");
      }
      setState({ kind: "result", report });
    } catch (error) {
      setState({
        kind: "error",
        message:
          error instanceof Error
            ? error.message
            : "The managed backup review is unavailable.",
      });
    } finally {
      running.current = false;
    }
  }

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <button
          className={settingsButtonClasses}
          type="button"
          aria-describedby={noteId}
          disabled={
            disabled || Boolean(disabledReason) || state.kind === "running"
          }
          onClick={() => void review()}
        >
          Review older backups…
        </button>
      </div>
      <p className="wrap-anywhere text-[0.8125rem] text-muted" id={noteId}>
        {disabledReason ??
          "Read-only review for the open bank in the current managed backup folder."}
      </p>
      {state.kind === "running" && (
        <p className="wrap-anywhere text-[0.8125rem] text-muted" role="status">
          Verifying managed backups…
        </p>
      )}
      {state.kind === "error" && (
        <p className="wrap-anywhere text-[0.8125rem] text-muted" role="alert">
          {state.message}
        </p>
      )}
      {state.kind === "result" && (
        <EditorDialog
          title="Older managed backups"
          onDismiss={() => setState({ kind: "idle" })}
        >
          <RetentionReviewContents report={state.report} />
        </EditorDialog>
      )}
    </>
  );
}

export function RetentionReviewContents({
  report,
}: {
  report: RetentionReview;
}) {
  return (
    <>
      <p>
        Read-only review of the managed folder chosen when the review started.
      </p>
      <p>
        {report.verifiedCount} verified · {report.retainedCount} newest to keep
        · {report.olderCount} older
      </p>
      {report.missingCount + report.unverifiedCount + report.rejectedMetadata >
        0 && (
        <p>
          Skipped: {report.missingCount} missing, {report.unverifiedCount}{" "}
          unverified, {report.rejectedMetadata} invalid records.
        </p>
      )}
      {report.olderCount === 0 ? (
        <p>No older verified backups for this bank.</p>
      ) : (
        <>
          {report.older.length < report.olderCount && (
            <p>
              Showing {report.older.length} of {report.olderCount} older
              backups.
            </p>
          )}
          <ul
            aria-label="Older verified backups"
            className="max-h-52 overflow-auto"
          >
            {report.older.map((backup) => (
              <li key={backup.name} className="break-all py-1">
                <p>{backup.name}</p>
                <p className="wrap-anywhere text-[0.8125rem] text-muted">
                  {backup.timestampUtc} UTC ·{" "}
                  {backup.byteLength.toLocaleString()} bytes
                </p>
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}
