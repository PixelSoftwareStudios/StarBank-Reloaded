import { useEffect, useId, useRef } from "react";
import {
  buttonClasses,
  dialogClasses,
  dialogHeaderClasses,
} from "../shared/control-styles";
import type { EditorDraft } from "./index";

export type DraftSwitchKind = EditorDraft["kind"] | "simple";

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
  simple: {
    title: "Simple draft needs review",
    repair: "Review edits",
    review: "Review Simple View",
  },
} satisfies Record<
  DraftSwitchKind,
  { title: string; repair: string; review: string }
>;

export function DraftSwitchDialog({
  kind,
  message,
  repairable,
  onDecision,
}: {
  kind: DraftSwitchKind;
  message: string;
  repairable: boolean;
  onDecision: (decision: DraftDecision) => void;
}) {
  const copy = draftCopy[kind];
  const heading = useId();
  const description = useId();
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
    return () => element?.close();
  }, []);

  function close(next: DraftDecision) {
    dialog.current?.close();
    onDecision(next);
  }

  return (
    <dialog
      className={dialogClasses}
      aria-labelledby={heading}
      aria-describedby={description}
      ref={dialog}
      onCancel={(event) => {
        event.preventDefault();
        close("cancel");
      }}
    >
      <header className={dialogHeaderClasses}>
        <h2 className="font-semibold" id={heading}>
          {copy.title}
        </h2>
      </header>
      <p id={description}>{message}</p>
      <div className="mt-3 flex flex-wrap justify-end gap-2">
        <button
          className={buttonClasses()}
          type="button"
          onClick={() => close("cancel")}
        >
          Cancel
        </button>
        <button
          className={buttonClasses()}
          type="button"
          onClick={() => close("discard")}
        >
          Discard and switch
        </button>
        <button
          className={buttonClasses("primary")}
          type="button"
          onClick={() => close("repair")}
        >
          {repairable ? copy.repair : copy.review}
        </button>
      </div>
    </dialog>
  );
}
