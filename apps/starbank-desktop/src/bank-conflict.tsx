import { useEffect, useId, useRef, useState } from "react";
import type { ConflictReview } from "./transport";
import { XmlComparisonView } from "./xml-comparison";

export interface BankConflictPrompt {
  compare: () => Promise<ConflictReview>;
  reopen: (id: string) => Promise<void>;
  saveAs?: () => Promise<void>;
  cancel: () => void;
  messageFor: (error: unknown) => string;
}

export function BankConflictDialog({
  compare,
  reopen,
  saveAs,
  cancel,
  messageFor,
}: BankConflictPrompt) {
  const dialog = useRef<HTMLDialogElement>(null);
  const running = useRef(false);
  const heading = useId();
  const [review, setReview] = useState<ConflictReview>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  async function run(operation: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError(undefined);
    try {
      await operation();
    } catch (error) {
      if (error === "conflict_review_changed") setReview(undefined);
      setError(messageFor(error));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  function loadReview() {
    setReview(undefined);
    void run(async () => {
      setReview(await compare());
    });
  }

  return (
    <dialog
      ref={dialog}
      aria-labelledby={heading}
      aria-busy={busy}
      className="fixed inset-0 m-auto max-h-[calc(100%-2rem)] w-[min(48rem,calc(100%-2rem))] overflow-auto rounded border border-neutral-400 bg-white p-4 text-sm text-neutral-900 shadow-lg backdrop:bg-black/30 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-100"
      onCancel={(event) => {
        event.preventDefault();
        if (!running.current) cancel();
      }}
    >
      <h2 id={heading} className="mb-2 font-semibold">
        Bank changed on disk
      </h2>
      <p>
        Your editor is still open. Compare with the file before reopening it.
      </p>
      {review && (
        <XmlComparisonView
          comparison={{
            firstDifferenceLine: review.firstDifferenceLine,
            startLine: review.startLine,
            before: review.editor,
            after: review.onDisk,
          }}
          beforeTitle="In the editor"
          afterTitle="On disk"
          matchingText="The XML text matches. The file bytes or metadata differ."
        />
      )}
      {error && (
        <p className="mt-3" role="alert">
          {error}
        </p>
      )}
      <div className="mt-4 flex flex-wrap justify-end gap-2">
        <button
          type="button"
          className="editor-button"
          onClick={cancel}
          disabled={busy}
        >
          Keep Editing
        </button>
        <button
          type="button"
          className="editor-button"
          onClick={loadReview}
          disabled={busy}
        >
          {review ? "Compare Again" : "Compare"}
        </button>
        <button
          type="button"
          className="editor-button"
          disabled={busy || !review}
          onClick={() => {
            if (review) void run(() => reopen(review.id));
          }}
        >
          Reopen from Disk…
        </button>
        {saveAs && (
          <button
            type="button"
            className="editor-button"
            onClick={() => void run(saveAs)}
            disabled={busy}
          >
            Save As…
          </button>
        )}
      </div>
    </dialog>
  );
}
