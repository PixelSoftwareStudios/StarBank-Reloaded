import { useEffect, useRef, useState } from "react";
import type { BackupChoices, BackupRestoreReview } from "./transport";
import { XmlComparisonView } from "./xml-comparison";

const description = {
  verified: "Verified",
  missing: "Missing",
  unverified: "Unverified",
};

export type BackupAction = { kind: "restore" | "export"; id: number };

export interface BackupPrompt {
  choices: BackupChoices;
  reviewRestore?: (id: number) => Promise<BackupRestoreReview>;
  select: (action?: BackupAction) => void;
}

export function BackupPicker({
  choices,
  reviewRestore,
  select,
  messageFor,
}: BackupPrompt & {
  messageFor: (error: unknown) => string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [selected, setSelected] = useState(
    () => choices.records.find((record) => record.health === "verified")?.id,
  );
  const [review, setReview] = useState<BackupRestoreReview>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  async function submit(id: number) {
    if (loading) return;
    if (!reviewRestore) {
      select({ kind: "export", id });
      return;
    }
    if (review) {
      select({ kind: "restore", id });
      return;
    }
    setLoading(true);
    setError(undefined);
    try {
      setReview(await reviewRestore(id));
    } catch (error) {
      setError(messageFor(error));
    } finally {
      setLoading(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      aria-labelledby="backup-picker-title"
      aria-busy={loading}
      className="fixed inset-0 m-auto max-h-[calc(100%-2rem)] w-[min(36rem,calc(100%-2rem))] overflow-auto rounded border border-neutral-400 bg-white p-4 text-sm text-neutral-900 shadow-lg backdrop:bg-black/30 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-100"
      onCancel={(event) => {
        event.preventDefault();
        if (!loading) select();
      }}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (selected !== undefined) void submit(selected);
        }}
      >
        <h2 id="backup-picker-title" className="mb-3 font-semibold">
          {reviewRestore ? "Restore Bank Backup" : "Bank Backups"}
        </h2>
        {review ? (
          <>
            <p>{review.description}</p>
            <XmlComparisonView
              comparison={review.comparison}
              beforeTitle="Current file"
              afterTitle="Backup"
              matchingText="The XML text matches."
            />
          </>
        ) : choices.records.length === 0 ? (
          <p>No recorded backups were found for this bank.</p>
        ) : (
          <fieldset
            className="max-h-72 space-y-1 overflow-auto"
            disabled={loading}
          >
            <legend className="sr-only">Bank backups</legend>
            {choices.records.map((record) => (
              <label
                key={record.id}
                className="flex gap-2 rounded border border-neutral-400 p-2 dark:border-neutral-600"
              >
                <input
                  type="radio"
                  name="backup"
                  checked={selected === record.id}
                  disabled={record.health !== "verified"}
                  onChange={() => {
                    setSelected(record.id);
                    setError(undefined);
                  }}
                />
                <span className="min-w-0">
                  <span className="block break-all">{record.name}</span>
                  <span className="text-xs">
                    {record.timestamp} UTC ·{" "}
                    {record.placement === "adjacent"
                      ? "Bank folder"
                      : "Managed"}{" "}
                    · {description[record.health]}
                  </span>
                </span>
              </label>
            ))}
          </fieldset>
        )}
        {!review && choices.unavailableLocations > 0 && (
          <p className="mt-2">Some managed backup folders are unavailable.</p>
        )}
        {!review && choices.rejectedMetadata > 0 ? (
          <p className="mt-2">
            Some backup records could not be read or verified.
          </p>
        ) : null}
        {!review && selected !== undefined && (
          <p className="mt-2">
            Save a copy to a new file with the backup’s original bytes and
            signature.
          </p>
        )}
        {error && (
          <p className="mt-2" role="alert">
            {error}
          </p>
        )}
        <div className="mt-4 flex flex-wrap justify-end gap-2">
          <button
            type="button"
            className="rounded border px-3 py-1"
            disabled={loading}
            onClick={() => select()}
          >
            Cancel
          </button>
          {review && (
            <button
              type="button"
              className="rounded border px-3 py-1"
              onClick={() => setReview(undefined)}
            >
              Back
            </button>
          )}
          {reviewRestore && !review && (
            <button
              type="button"
              className="rounded border px-3 py-1 disabled:opacity-40"
              disabled={selected === undefined || loading}
              onClick={() => {
                if (selected !== undefined)
                  select({ kind: "export", id: selected });
              }}
            >
              Save a Copy…
            </button>
          )}
          <button
            type="submit"
            className="rounded border px-3 py-1 disabled:opacity-40"
            disabled={selected === undefined || loading}
          >
            {review
              ? "Restore…"
              : reviewRestore
                ? loading
                  ? "Reviewing…"
                  : "Review Restore…"
                : "Save a Copy…"}
          </button>
        </div>
      </form>
    </dialog>
  );
}
