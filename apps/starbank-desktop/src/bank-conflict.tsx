import { useEffect, useId, useRef, useState } from "react";
import type { ConflictExcerpt, ConflictReview } from "./transport";

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
        <div className="mt-3">
          <p role="status">
            {review.firstDifferenceLine === null
              ? "The XML text matches. The file bytes or metadata differ."
              : `First XML difference at line ${review.firstDifferenceLine}. Both excerpts start at line ${review.startLine}.`}
          </p>
          <div className="mt-2 grid min-w-0 gap-2 sm:grid-cols-2">
            <Excerpt title="In the editor" excerpt={review.editor} />
            <Excerpt title="On disk" excerpt={review.onDisk} />
          </div>
        </div>
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

function Excerpt({
  title,
  excerpt,
}: {
  title: string;
  excerpt: ConflictExcerpt;
}) {
  return (
    <div className="min-w-0 rounded border border-neutral-400 p-2 dark:border-neutral-600">
      <h3 className="mb-1 font-semibold">{title}</h3>
      {excerpt.omitsStart && <p className="text-xs">Earlier text omitted</p>}
      <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all text-xs">
        {excerpt.text}
      </pre>
      {excerpt.omitsEnd && <p className="text-xs">Later text omitted</p>}
    </div>
  );
}
