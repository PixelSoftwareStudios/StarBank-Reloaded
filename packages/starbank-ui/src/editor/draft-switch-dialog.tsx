import { useEffect, useId, useRef } from "react";
import type { EditorDraft } from "starbank-editor";

type DraftDecision = "repair" | "discard" | "cancel";

const draftCopy = {
  raw: {
    title: "Raw draft could not be applied",
    repair: "Repair XML",
    review: "Review Raw View",
  },
  table: {
    title: "Table draft could not be applied",
    repair: "Repair edits",
    review: "Review Table View",
  },
} satisfies Record<
  EditorDraft["kind"],
  { title: string; repair: string; review: string }
>;

export function DraftSwitchDialog({
  kind,
  message,
  repairable,
  onDecision,
}: {
  kind: EditorDraft["kind"];
  message: string;
  repairable: boolean;
  onDecision: (decision: DraftDecision) => void;
}) {
  const copy = draftCopy[kind];
  const heading = useId();
  const description = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const decision = useRef<DraftDecision>("cancel");

  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
  }, []);

  function close(next: DraftDecision) {
    decision.current = next;
    dialog.current?.close();
  }

  return (
    <dialog
      className="editor-dialog"
      aria-labelledby={heading}
      aria-describedby={description}
      ref={dialog}
      onClose={() => onDecision(decision.current)}
    >
      <header className="editor-dialog-header">
        <h2 id={heading}>{copy.title}</h2>
      </header>
      <p id={description}>{message}</p>
      <div className="mt-3 flex flex-wrap justify-end gap-2">
        <button
          className="editor-button"
          type="button"
          onClick={() => close("cancel")}
        >
          Cancel
        </button>
        <button
          className="editor-button"
          type="button"
          onClick={() => close("discard")}
        >
          Discard and switch
        </button>
        <button
          className="editor-button editor-primary"
          type="button"
          onClick={() => close("repair")}
        >
          {repairable ? copy.repair : copy.review}
        </button>
      </div>
    </dialog>
  );
}
