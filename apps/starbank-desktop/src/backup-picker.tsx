import { useEffect, useRef, useState } from "react";
import type { BackupChoices } from "./transport";

const description = {
  verified: "Verified",
  missing: "Missing",
  unverified: "Unverified",
};

export type BackupAction = { kind: "restore" | "export"; id: number };

export function BackupPicker({
  choices,
  canRestore,
  select,
}: {
  choices: BackupChoices;
  canRestore: boolean;
  select: (action?: BackupAction) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [selected, setSelected] = useState(
    () => choices.records.find((record) => record.health === "verified")?.id,
  );

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  return (
    <dialog
      ref={dialog}
      aria-labelledby="backup-picker-title"
      className="fixed inset-0 m-auto w-[min(36rem,calc(100%-2rem))] rounded border border-neutral-400 bg-white p-4 text-sm text-neutral-900 shadow-lg backdrop:bg-black/30 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-100"
      onCancel={(event) => {
        event.preventDefault();
        select();
      }}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (selected !== undefined)
            select({ kind: canRestore ? "restore" : "export", id: selected });
        }}
      >
        <h2 id="backup-picker-title" className="mb-3 font-semibold">
          {canRestore ? "Restore Bank Backup" : "Bank Backups"}
        </h2>
        {choices.records.length === 0 ? (
          <p>No recorded backups were found for this bank.</p>
        ) : (
          <fieldset className="max-h-72 space-y-1 overflow-auto">
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
                  onChange={() => setSelected(record.id)}
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
        {choices.unavailableLocations > 0 && (
          <p className="mt-2">Some managed backup folders are unavailable.</p>
        )}
        {choices.rejectedMetadata > 0 ? (
          <p className="mt-2">
            Some backup records could not be read or verified.
          </p>
        ) : null}
        {selected !== undefined && (
          <p className="mt-2">
            Save a copy to a new file with the backup’s original bytes and
            signature.
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            className="rounded border px-3 py-1"
            onClick={() => select()}
          >
            Cancel
          </button>
          {canRestore && (
            <button
              type="button"
              className="rounded border px-3 py-1 disabled:opacity-40"
              disabled={selected === undefined}
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
            disabled={selected === undefined}
          >
            {canRestore ? "Continue" : "Save a Copy…"}
          </button>
        </div>
      </form>
    </dialog>
  );
}
