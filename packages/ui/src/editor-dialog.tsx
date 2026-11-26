import { type ReactNode, useEffect, useId, useRef } from "react";

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
      className="editor-dialog"
      aria-labelledby={heading}
      onClose={onDismiss}
      ref={ref}
    >
      <header className="editor-dialog-header">
        <h2 id={heading}>{title}</h2>
        <button
          className="editor-button"
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
