import { useEffect, useId, useRef, useState } from "react";
import { writeBankClipboard } from "starbank-ui/browser";
import {
  buttonClasses,
  dialogClasses,
  dialogHeaderClasses,
  focusOutlineClasses,
} from "starbank-ui/controls";

export function BankClipboardDialog({
  xml,
  close,
}: {
  xml: string;
  close: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  const [message, setMessage] = useState("Select and copy the prepared XML.");
  const canRetry = typeof navigator.clipboard?.writeText === "function";

  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
  }, []);

  return (
    <dialog
      aria-labelledby={title}
      className={dialogClasses}
      onClose={close}
      ref={dialog}
    >
      <header className={dialogHeaderClasses}>
        <h2 id={title} className="font-semibold">
          Copy bank XML
        </h2>
        <button
          className={buttonClasses()}
          type="button"
          onClick={() => dialog.current?.close()}
        >
          Close
        </button>
      </header>
      <p role="status">{message}</p>
      <textarea
        aria-label="Prepared bank XML"
        className={`mt-2 block w-full rounded-control border border-control-edge bg-field p-2 font-mono ${focusOutlineClasses}`}
        readOnly
        rows={8}
        spellCheck={false}
        value={xml}
        onFocus={(event) => event.currentTarget.select()}
        onCopy={(event) => {
          const field = event.currentTarget;
          if (
            field.selectionStart === 0 &&
            field.selectionEnd === field.value.length
          ) {
            // Use the prepared text even if the textarea normalized line endings.
            event.clipboardData.setData("text/plain", xml);
            event.preventDefault();
          }
        }}
      />
      {canRetry && (
        <button
          className={`${buttonClasses()} mt-2`}
          type="button"
          onClick={() => {
            void writeBankClipboard(xml).then(
              () => setMessage("Bank XML copied."),
              () =>
                setMessage(
                  "Clipboard access was denied. Select and copy the XML.",
                ),
            );
          }}
        >
          Copy XML
        </button>
      )}
    </dialog>
  );
}
