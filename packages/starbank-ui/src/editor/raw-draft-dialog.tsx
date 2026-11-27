import { useEffect, useId, useRef } from "react";

export type RawDraftDecision = "repair" | "discard" | "cancel";

export function RawDraftDialog({
  message,
  repairable,
  onDecision,
}: {
  message: string;
  repairable: boolean;
  onDecision: (decision: RawDraftDecision) => void;
}) {
  const heading = useId();
  const description = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const decision = useRef<RawDraftDecision>("cancel");

  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
  }, []);

  function close(next: RawDraftDecision) {
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
        <h2 id={heading}>Raw draft could not be applied</h2>
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
          {repairable ? "Repair XML" : "Review Raw View"}
        </button>
      </div>
    </dialog>
  );
}
