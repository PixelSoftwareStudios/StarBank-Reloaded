import { type ReactNode, useEffect, useId, useRef } from "react";
import {
  buttonClasses,
  dialogClasses,
  dialogHeaderClasses,
} from "../shared/control-styles";

export function EditorDialog({
  title,
  onDismiss,
  children,
}: {
  title: string;
  onDismiss: () => void;
  children: ReactNode;
}) {
  const heading = useId();
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  return (
    <dialog
      className={dialogClasses}
      aria-labelledby={heading}
      onClose={onDismiss}
      ref={ref}
    >
      <header className={dialogHeaderClasses}>
        <h2 className="font-semibold" id={heading}>
          {title}
        </h2>
        <button
          className={buttonClasses()}
          type="button"
          onClick={() => ref.current?.close()}
        >
          Close
        </button>
      </header>
      {children}
    </dialog>
  );
}
