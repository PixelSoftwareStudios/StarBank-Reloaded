import { useEffect, useId, useRef, useState } from "react";
import { buttonClasses } from "starbank-ui/controls";
import type {
  EngineSignatureStatus,
  EngineSigningContext,
} from "starbank-ui/engine/protocol";

type CheckedSignature = Exclude<EngineSignatureStatus, "missing_context">;

export interface ManualSigningPrompt {
  name: string;
  sourceStatus?: string;
  purpose?: "editor" | "signed_copy";
  check: (context: EngineSigningContext) => Promise<CheckedSignature>;
  apply: (context: EngineSigningContext) => Promise<void>;
  dismiss: () => void;
  errorMessage: (error: unknown) => string;
}

const previewMessage: Record<CheckedSignature, string> = {
  unsigned: "This bank has no stored signature.",
  malformed: "The stored signature is malformed.",
  unsupported: "This bank contains data the signature checker cannot verify.",
  valid: "The stored signature matches this context.",
  invalid: "The stored signature does not match this context.",
};

const sourceMessage: Record<string, string> = {
  unsigned: "Unsigned",
  malformed: "Malformed signature",
  missing_context: "Signing context needed",
  unsupported: "Unsupported signature",
  valid: "Valid signature",
  invalid: "Invalid signature",
};

export function ManualSigningSheet({
  name,
  sourceStatus,
  purpose = "editor",
  check,
  apply,
  dismiss,
  errorMessage,
}: ManualSigningPrompt) {
  const dialog = useRef<HTMLDialogElement>(null);
  const title = useId();
  const summary = useId();
  const [context, setContext] = useState<EngineSigningContext>({
    author: "",
    player: "",
    bank_name: "",
  });
  const [working, setWorking] = useState(false);
  const [status, setStatus] = useState<CheckedSignature>();
  const [error, setError] = useState<string>();

  useEffect(() => {
    dialog.current?.showModal();
  }, []);

  function update(field: keyof EngineSigningContext, value: string) {
    setContext((previous) => ({ ...previous, [field]: value }));
    setStatus(undefined);
    setError(undefined);
  }

  async function run(kind: "check" | "apply") {
    if (working) return;
    setWorking(true);
    setError(undefined);
    setStatus(undefined);
    try {
      if (kind === "check") {
        setStatus(await check(context));
      } else {
        await apply(context);
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setWorking(false);
    }
  }

  const actionLabel =
    purpose === "signed_copy" ? "Save signed copy…" : "Re-sign in editor";

  return (
    <dialog
      ref={dialog}
      aria-labelledby={title}
      aria-describedby={summary}
      className="fixed inset-0 m-auto max-h-[calc(100%-2rem)] w-[min(30rem,calc(100%-2rem))] overflow-auto rounded border border-neutral-400 bg-white p-4 text-sm text-neutral-900 shadow-lg backdrop:bg-black/30 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-100"
      onCancel={(event) => {
        event.preventDefault();
        if (!working) dismiss();
      }}
    >
      <h2 id={title} className="mb-2 text-base font-semibold">
        Re-sign external bank
      </h2>
      <div id={summary} className="space-y-2">
        <p className="break-all">
          <span className="font-medium">{name}</span>
          {sourceStatus && sourceMessage[sourceStatus]
            ? ` · ${sourceMessage[sourceStatus]}`
            : ""}
        </p>
        <p>
          Supply the context this bank uses in StarCraft II.{" "}
          {purpose === "signed_copy"
            ? "The signed copy is saved to a new file."
            : "Re-signing updates the editor and can be undone."}
        </p>
      </div>
      <form
        className="mt-3 space-y-3"
        aria-busy={working}
        onSubmit={(event) => {
          event.preventDefault();
          void run("apply");
        }}
      >
        <fieldset disabled={working} className="grid gap-2">
          <legend className="sr-only">Signing context</legend>
          <label className="grid gap-1">
            Map author handle
            <input
              value={context.author}
              onChange={(event) => update("author", event.target.value)}
              autoCapitalize="none"
              autoComplete="off"
              spellCheck={false}
              required
              className="rounded border border-neutral-500 px-2 py-1"
            />
          </label>
          <label className="grid gap-1">
            Player handle
            <input
              value={context.player}
              onChange={(event) => update("player", event.target.value)}
              autoCapitalize="none"
              autoComplete="off"
              spellCheck={false}
              required
              className="rounded border border-neutral-500 px-2 py-1"
            />
          </label>
          <label className="grid gap-1">
            Bank name without extension
            <input
              value={context.bank_name}
              onChange={(event) => update("bank_name", event.target.value)}
              autoCapitalize="none"
              autoComplete="off"
              spellCheck={false}
              required
              className="rounded border border-neutral-500 px-2 py-1"
            />
          </label>
        </fieldset>
        {status ? <p role="status">{previewMessage[status]}</p> : null}
        {error ? <p role="alert">{error}</p> : null}
        <div className="flex flex-wrap justify-end gap-2">
          <button
            className={buttonClasses()}
            disabled={working}
            onClick={dismiss}
            type="button"
          >
            Cancel
          </button>
          <button
            className={buttonClasses()}
            type="button"
            disabled={working}
            onClick={(event) => {
              if (event.currentTarget.form?.reportValidity()) void run("check");
            }}
          >
            Check signature
          </button>
          <button
            className={buttonClasses("primary")}
            type="submit"
            disabled={working}
          >
            {working ? "Working…" : actionLabel}
          </button>
        </div>
      </form>
    </dialog>
  );
}
