import { useEffect, useId, useRef } from "react";
import {
  buttonClasses,
  dialogClasses,
  dialogHeaderClasses,
  focusOutlineClasses,
} from "starbank-ui/controls";
import type {
  WriteRecoveryEvidence,
  WriteRecoveryPath,
  WriteRecoveryReceipt,
} from "./write-recovery";

const operationLabel = {
  save_overwrite: "Save (Overwrite)",
  save_as: "Save As",
  restore: "Restore backup",
  export_copy: "Export copy",
  backup_copy: "Export opening backup",
  external_signed_copy: "Re-sign external bank",
} satisfies Record<WriteRecoveryReceipt["operation"], string>;

function outcomeText(receipt: WriteRecoveryReceipt) {
  if (receipt.outcome === "verified_needs_reopen") {
    return "StarBank verified the write on disk. Reopen the destination to update the editor.";
  }
  if (receipt.outcome === "live_unchanged") {
    return "StarBank stopped during backup preparation. The destination was left unchanged.";
  }
  return receipt.affectsActive
    ? "StarBank could not verify this write. Reopen and inspect this destination before trying to overwrite it."
    : "StarBank could not verify the write to the new destination. The source bank was left unchanged.";
}

function RecordedPath({
  label,
  path,
}: {
  label: string;
  path: WriteRecoveryPath;
}) {
  const description = useId();
  const hasWarning = path.truncated || path.lossy;
  return (
    <div>
      <label className="block font-medium">
        {label}
        <textarea
          aria-describedby={hasWarning ? description : undefined}
          className={`mt-1 block w-full rounded-control border border-control-edge bg-field px-2 py-1 font-mono ${focusOutlineClasses}`}
          readOnly
          rows={2}
          spellCheck={false}
          value={path.display}
          onCopy={(event) => {
            const field = event.currentTarget;
            if (
              field.selectionStart === 0 &&
              field.selectionEnd === field.value.length
            ) {
              // Preserve the recorded text even if the textarea normalized line endings.
              event.clipboardData.setData("text/plain", path.display);
              event.preventDefault();
            }
          }}
        />
      </label>
      {hasWarning && (
        <div className="mt-1 text-muted" id={description}>
          {path.truncated && <p>The recorded path was shortened.</p>}
          {path.lossy && <p>Some path characters could not be represented.</p>}
        </div>
      )}
    </div>
  );
}

export function WriteRecoveryDetails({
  evidence,
  close,
}: {
  evidence: WriteRecoveryEvidence;
  close: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  const summary = useId();

  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
  }, []);

  const receipt = evidence.kind === "receipt" ? evidence.receipt : undefined;
  return (
    <dialog
      ref={dialog}
      aria-labelledby={title}
      aria-describedby={summary}
      className={dialogClasses}
      onClose={close}
    >
      <header className={dialogHeaderClasses}>
        <h2 className="font-semibold" id={title}>
          Write recovery details
        </h2>
        <button
          type="button"
          className={buttonClasses()}
          onClick={() => dialog.current?.close()}
        >
          Close
        </button>
      </header>
      <p id={summary}>
        {receipt
          ? outcomeText(receipt)
          : "StarBank could not read the write recovery receipt. The destination state could not be verified."}
      </p>
      {receipt && (
        <div className="mt-3 space-y-3">
          <p>
            <span className="font-medium">Operation</span>{" "}
            {operationLabel[receipt.operation]}
          </p>
          <p className="text-muted">
            These locations were recorded during the operation. StarBank has not
            checked whether the files still exist there.
          </p>
          <RecordedPath label="Destination path" path={receipt.destination} />
          {receipt.recovery.kind === "recorded" ? (
            <RecordedPath
              label="Recorded recovery location"
              path={receipt.recovery.path}
            />
          ) : (
            <p>No recovery location was recorded.</p>
          )}
          {receipt.backup.kind === "not_created" ? (
            <p>No backup was created during the operation.</p>
          ) : (
            <div>
              <RecordedPath
                label={
                  receipt.backup.kind === "verified"
                    ? "Verified backup location"
                    : "Unverified backup location"
                }
                path={receipt.backup.path}
              />
              <p className="mt-1 text-muted">
                {receipt.backup.kind === "verified"
                  ? "StarBank verified the backup before attempting the write."
                  : "StarBank could not verify the backup or its metadata."}
              </p>
            </div>
          )}
          <div>
            <h3 className="font-medium">Reason reported</h3>
            <p className="whitespace-pre-wrap wrap-anywhere">
              {receipt.reason}
            </p>
            {receipt.reasonTruncated && (
              <p className="text-muted">The reported reason was shortened.</p>
            )}
          </div>
        </div>
      )}
    </dialog>
  );
}
