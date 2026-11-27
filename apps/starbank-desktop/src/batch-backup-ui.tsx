import { useEffect, useId, useRef, useState } from "react";
import { Dropdown, ToolsMenuTrigger } from "starbank-ui";
import {
  focusOutlineClasses,
  menuContentClasses,
  menuItemClasses,
} from "starbank-ui/controls";

export type BackupScope = { kind: "all" } | { kind: "map"; id: string };
export type BackupPlacement = "managed" | "adjacent";

export interface BatchBackupRequest {
  scope: BackupScope;
  placement: BackupPlacement;
}

export type BatchBackupOutcome =
  | "created"
  | "source_unavailable"
  | "rejected"
  | "needs_review"
  | "cancelled"
  | "inventory_changed";

export interface BatchBackupReport {
  entries: readonly {
    bankId: string;
    name: string;
    outcome: BatchBackupOutcome;
    backupName: string | null;
  }[];
  created: number;
  failed: number;
  skipped: number;
}

export type BackupAvailability =
  | { kind: "unavailable"; reason: string }
  | { kind: "ready"; selectedMap?: { id: string; name: string } };

interface BatchBackupToolsProps {
  availability: BackupAvailability;
  initialPlacement?: BackupPlacement;
  onResignExternal?: () => void;
  onReviewWriteRecovery?: () => void;
  externalSigningUnavailable?: string;
  onStart: (request: BatchBackupRequest) => Promise<BatchBackupReport | null>;
  onCancel: () => Promise<void>;
}

type CancelState = "available" | "requesting" | "requested" | "failed";

type DialogState =
  | { kind: "closed" }
  | { kind: "placement"; scope: BackupScope; name: string }
  | { kind: "running"; cancel: CancelState }
  | { kind: "report"; report: BatchBackupReport }
  | { kind: "failed"; message: string };

const REPORT_PAGE_SIZE = 60;
const batchControlClasses = `rounded-[0.3rem] border border-line bg-field px-2 py-[0.35rem] text-ink ${focusOutlineClasses}`;
const batchButtonClasses = `${batchControlClasses} disabled:cursor-not-allowed disabled:opacity-55`;
const reportCellClasses =
  "border-b border-line p-[0.3rem] text-start align-top [overflow-wrap:anywhere]";
const actionRowClasses = "flex flex-wrap items-center justify-end gap-[0.4rem]";
const OUTCOME_LABEL: Record<BatchBackupOutcome, string> = {
  created: "Created",
  source_unavailable: "Bank unavailable",
  rejected: "Backup rejected",
  needs_review: "Needs review",
  cancelled: "Canceled",
  inventory_changed: "Inventory changed",
};

function describeFailure(cause: unknown) {
  const code =
    typeof cause === "object" && cause && "inventory" in cause
      ? cause.inventory
      : cause;
  switch (code) {
    case "unsupported":
      return "Native batch backup is unavailable on this platform.";
    case "no_banks":
      return "No bank files were found in the selected scope.";
    case "unknown_map":
      return "The selected map is no longer available.";
    case "too_many_banks":
      return "The selected scope exceeds the batch backup limit.";
    case "busy":
      return "Another file operation is already in progress.";
    default:
      return "The batch backup could not be started or completed.";
  }
}

function Report({ report }: { report: BatchBackupReport }) {
  const [offset, setOffset] = useState(0);
  const end = Math.min(offset + REPORT_PAGE_SIZE, report.entries.length);
  const canceled = report.entries.some(
    ({ outcome }) => outcome === "cancelled",
  );

  return (
    <>
      <p role="status">
        Created {report.created} · Failed {report.failed} · Skipped{" "}
        {report.skipped}
      </p>
      {canceled && <p>Backup canceled. Files already created were kept.</p>}
      {report.entries.length > 0 && (
        <>
          <div className="batch-backup-report max-h-[min(46dvh,16rem)] overflow-auto">
            <table
              aria-label="Bank backup results"
              className="w-full table-fixed border-collapse"
            >
              <thead className="sticky top-0 bg-surface">
                <tr>
                  <th scope="col" className={reportCellClasses}>
                    Bank
                  </th>
                  <th scope="col" className={reportCellClasses}>
                    Outcome
                  </th>
                </tr>
              </thead>
              <tbody>
                {report.entries.slice(offset, end).map((entry) => (
                  <tr key={entry.bankId}>
                    <th scope="row" className={reportCellClasses}>
                      {entry.name}
                    </th>
                    <td className={reportCellClasses}>
                      {OUTCOME_LABEL[entry.outcome] ?? "Unknown outcome"}
                      {entry.backupName && (
                        <small className="block text-muted">
                          {entry.backupName}
                        </small>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {report.entries.length > REPORT_PAGE_SIZE && (
            <div className={`batch-backup-pagination ${actionRowClasses}`}>
              <p role="status" className="me-auto">
                {offset + 1}–{end} of {report.entries.length}
              </p>
              <button
                className={batchButtonClasses}
                type="button"
                disabled={offset === 0}
                onClick={() =>
                  setOffset(Math.max(0, offset - REPORT_PAGE_SIZE))
                }
              >
                Previous
              </button>
              <button
                className={batchButtonClasses}
                type="button"
                disabled={end === report.entries.length}
                onClick={() => setOffset(end)}
              >
                Next
              </button>
            </div>
          )}
        </>
      )}
    </>
  );
}

export function BatchBackupTools({
  availability,
  initialPlacement = "managed",
  onResignExternal,
  onReviewWriteRecovery,
  externalSigningUnavailable,
  onStart,
  onCancel,
}: BatchBackupToolsProps) {
  const titleId = useId();
  const availabilityId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const active = useRef(false);
  const cancelPending = useRef(false);
  const generation = useRef(0);
  const [state, setState] = useState<DialogState>({ kind: "closed" });
  const [placement, setPlacement] = useState<BackupPlacement>(initialPlacement);
  const ready = availability.kind === "ready";
  const selectedMap = ready ? availability.selectedMap : undefined;
  const blockedMessage = ready ? undefined : availability.reason;
  const chooseCurrentScope =
    state.kind === "placement" &&
    (state.scope.kind === "all" || state.scope.id === selectedMap?.id);
  const dialogOpen = state.kind !== "closed";

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (dialogOpen && !element.open) element.showModal();
    if (!dialogOpen && element.open) {
      element.close();
      trigger.current?.focus();
    }
  }, [dialogOpen]);

  const dialogStage = state.kind;
  useEffect(() => {
    if (dialogStage === "report" || dialogStage === "failed")
      heading.current?.focus();
  }, [dialogStage]);

  function choose(scope: BackupScope, name: string) {
    if (!ready || active.current) return;
    setPlacement(initialPlacement);
    setState({ kind: "placement", scope, name });
  }

  async function start(scope: BackupScope) {
    if (!ready || active.current || !chooseCurrentScope) return;
    active.current = true;
    generation.current += 1;
    cancelPending.current = false;
    setState({ kind: "running", cancel: "available" });
    try {
      const report = await onStart({ scope, placement });
      setState(report ? { kind: "report", report } : { kind: "closed" });
    } catch (cause) {
      setState({ kind: "failed", message: describeFailure(cause) });
    } finally {
      active.current = false;
    }
  }

  async function cancel() {
    if (
      !active.current ||
      cancelPending.current ||
      state.kind !== "running" ||
      state.cancel === "requested"
    )
      return;
    const attempt = generation.current;
    cancelPending.current = true;
    setState({ kind: "running", cancel: "requesting" });
    try {
      await onCancel();
      if (generation.current === attempt) {
        setState((current) =>
          current.kind === "running"
            ? { kind: "running", cancel: "requested" }
            : current,
        );
      }
    } catch {
      if (generation.current === attempt) {
        setState((current) =>
          current.kind === "running"
            ? { kind: "running", cancel: "failed" }
            : current,
        );
      }
    } finally {
      if (generation.current === attempt) cancelPending.current = false;
    }
  }

  const title =
    state.kind === "placement"
      ? state.name
      : state.kind === "running"
        ? "Creating backups"
        : state.kind === "report"
          ? "Backup results"
          : "Batch backup";

  return (
    <>
      <span className="inline-flex">
        <Dropdown.Root modal={false}>
          <ToolsMenuTrigger
            ref={trigger}
            descriptionId={blockedMessage ? availabilityId : undefined}
            disabled={dialogOpen}
          />
          <Dropdown.Portal>
            <Dropdown.Content
              side="bottom"
              align="end"
              sideOffset={5}
              aria-label="Bank tools"
              className={`${menuContentClasses} max-w-[17rem]!`}
            >
              {blockedMessage && (
                <p className="px-2 py-[0.3rem] text-[0.8125rem] text-muted">
                  {blockedMessage}
                </p>
              )}
              <Dropdown.Item className={menuItemClasses} disabled>
                Unprotect external map file…
              </Dropdown.Item>
              <Dropdown.Item
                className={menuItemClasses}
                disabled={!onResignExternal}
                onSelect={() => onResignExternal?.()}
              >
                Re-sign external bank…
              </Dropdown.Item>
              <Dropdown.Item
                className={menuItemClasses}
                disabled={!ready}
                onSelect={() =>
                  choose({ kind: "all" }, "Back up all bank files")
                }
              >
                Back up all bank files
              </Dropdown.Item>
              <Dropdown.Item
                className={menuItemClasses}
                disabled={!selectedMap}
                onSelect={() => {
                  if (selectedMap)
                    choose(
                      { kind: "map", id: selectedMap.id },
                      `Back up ${selectedMap.name}`,
                    );
                }}
              >
                Back up selected map
              </Dropdown.Item>
              {onReviewWriteRecovery && (
                <Dropdown.Item
                  className={menuItemClasses}
                  onSelect={onReviewWriteRecovery}
                >
                  Review write recovery…
                </Dropdown.Item>
              )}
              <p className="px-2 py-[0.3rem] text-[0.8125rem] text-muted">
                Map repair is unavailable until archive writes are qualified.
              </p>
              {externalSigningUnavailable && (
                <p className="px-2 py-[0.3rem] text-[0.8125rem] text-muted">
                  {externalSigningUnavailable}
                </p>
              )}
              {ready && !selectedMap && (
                <p className="px-2 py-[0.3rem] text-[0.8125rem] text-muted">
                  Select a map to back up its bank files.
                </p>
              )}
            </Dropdown.Content>
          </Dropdown.Portal>
        </Dropdown.Root>
        {blockedMessage && (
          <span className="sr-only" id={availabilityId}>
            {blockedMessage}
          </span>
        )}
      </span>

      <dialog
        ref={dialog}
        className="batch-backup-dialog box-border m-auto max-h-[calc(100dvh_-_1.5rem)] w-[min(32rem,calc(100%_-_1.5rem))] overflow-auto rounded-lg border border-line bg-surface p-4 text-[0.875rem] text-ink open:grid open:gap-3 backdrop:bg-black/48"
        aria-labelledby={titleId}
        onCancel={(event) => {
          event.preventDefault();
          if (state.kind === "running") void cancel();
          else setState({ kind: "closed" });
        }}
      >
        <h2
          id={titleId}
          ref={heading}
          tabIndex={-1}
          className="text-[1.125rem] font-semibold [overflow-wrap:anywhere]"
        >
          {title}
        </h2>

        {state.kind === "placement" && (
          <>
            <label className="grid gap-[0.4rem]">
              Backup location
              <select
                className={batchControlClasses}
                value={placement}
                onChange={(event) =>
                  setPlacement(
                    event.currentTarget.value === "adjacent"
                      ? "adjacent"
                      : "managed",
                  )
                }
              >
                <option value="managed">Managed backups</option>
                <option value="adjacent">Next to the bank files</option>
              </select>
            </label>
            {(!ready || !chooseCurrentScope) && (
              <p role="status">
                The inventory or selected map changed. Choose the backup scope
                again.
              </p>
            )}
            <div className={actionRowClasses}>
              <button
                className={batchButtonClasses}
                type="button"
                onClick={() => setState({ kind: "closed" })}
              >
                Close
              </button>
              <button
                className={batchButtonClasses}
                type="button"
                disabled={!ready || !chooseCurrentScope}
                onClick={() => void start(state.scope)}
              >
                Back up
              </button>
            </div>
          </>
        )}

        {state.kind === "running" && (
          <>
            <p role="status">
              {state.cancel === "requested"
                ? "Cancellation requested…"
                : "Waiting for backup results…"}
            </p>
            {state.cancel === "failed" && (
              <p role="alert">
                Cancellation could not be requested. You can try again.
              </p>
            )}
            <div className={actionRowClasses}>
              <button
                className={batchButtonClasses}
                type="button"
                disabled={
                  state.cancel === "requesting" || state.cancel === "requested"
                }
                onClick={() => void cancel()}
              >
                Cancel backup
              </button>
            </div>
          </>
        )}

        {state.kind === "report" && <Report report={state.report} />}
        {state.kind === "failed" && <p role="alert">{state.message}</p>}
        {(state.kind === "report" || state.kind === "failed") && (
          <div className={actionRowClasses}>
            <button
              className={batchButtonClasses}
              type="button"
              onClick={() => setState({ kind: "closed" })}
            >
              Close
            </button>
          </div>
        )}
      </dialog>
    </>
  );
}
