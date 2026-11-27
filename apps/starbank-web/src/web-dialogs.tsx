import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import type { BankDownload } from "starbank-ui/browser";
import { buttonClasses } from "starbank-ui/controls";
import {
  bankFilenameStem,
  type EngineSigningContext,
} from "starbank-ui/engine/protocol";

const actionsClasses = "mt-4 flex flex-wrap justify-end gap-2";
const fieldLabelClasses = "flex flex-col gap-[0.35rem]";
const fieldClasses = "w-full rounded-sm border border-line px-2 py-[0.35rem]";
const bodyTextClasses = "my-[0.65rem]";

interface DownloadPreparation {
  readonly download: BankDownload;
  readonly message?: string;
}

export function ExportDialog({
  name,
  initialSign = false,
  prepare,
  prepareBackup,
  continueAfterDownload,
  dismiss,
}: {
  name: string;
  initialSign?: boolean;
  prepare: (
    name: string,
    context?: EngineSigningContext,
  ) => Promise<DownloadPreparation>;
  prepareBackup?: () => Promise<DownloadPreparation>;
  continueAfterDownload?: () => void;
  dismiss: () => void;
}) {
  const [filename, setFilename] = useState(name);
  const [sign, setSign] = useState(initialSign);
  const [author, setAuthor] = useState("");
  const [player, setPlayer] = useState("");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [prepared, setPrepared] = useState<DownloadPreparation>();
  const [downloadRequested, setDownloadRequested] = useState(false);
  const downloadLink = useRef<HTMLAnchorElement>(null);

  useEffect(() => {
    if (!prepared) return;
    downloadLink.current?.focus();
    return () => prepared.download.release();
  }, [prepared]);

  async function prepareCopy(
    makeDownload: () => Promise<DownloadPreparation>,
  ): Promise<void> {
    setWorking(true);
    setError("");
    setDownloadRequested(false);
    try {
      setPrepared(await makeDownload());
    } catch (failure) {
      setError(
        failure instanceof Error
          ? failure.message
          : "The download could not be prepared.",
      );
    } finally {
      setWorking(false);
    }
  }

  async function prepareEditedCopy(): Promise<DownloadPreparation> {
    const stem = bankFilenameStem(filename);
    if (!stem) throw new Error("Choose a filename ending in .SC2Bank.");
    const context = sign ? { author, player, bank_name: stem } : undefined;
    return prepare(filename, context);
  }

  return (
    <WebDialog title="Save a bank copy" close={dismiss} busy={working}>
      {prepared ? (
        <>
          {prepared.message && (
            <p className={bodyTextClasses} role="status">
              {prepared.message}
            </p>
          )}
          <div className={actionsClasses}>
            <button
              className={buttonClasses()}
              type="button"
              onClick={() => setPrepared(undefined)}
            >
              Back
            </button>
            <button className={buttonClasses()} type="button" onClick={dismiss}>
              {continueAfterDownload ? "Cancel" : "Close"}
            </button>
            <a
              ref={downloadLink}
              className={buttonClasses("primary")}
              href={prepared.download.href}
              download={prepared.download.name}
              onClick={
                continueAfterDownload
                  ? () => setDownloadRequested(true)
                  : undefined
              }
            >
              Download {prepared.download.name}
            </a>
            {continueAfterDownload && (
              <button
                className={buttonClasses("primary")}
                type="button"
                disabled={!downloadRequested}
                onClick={continueAfterDownload}
              >
                Continue after download
              </button>
            )}
          </div>
        </>
      ) : (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!working) void prepareCopy(prepareEditedCopy);
          }}
        >
          <fieldset disabled={working} className="flex flex-col gap-3">
            <label className={fieldLabelClasses}>
              Filename
              <input
                className={fieldClasses}
                value={filename}
                onChange={(event) => setFilename(event.target.value)}
                required
              />
            </label>
            <label className="flex items-center gap-[0.35rem]">
              <input
                type="checkbox"
                checked={sign}
                onChange={(event) => setSign(event.target.checked)}
              />
              Re-sign this copy
            </label>
            {sign && (
              <>
                <label className={fieldLabelClasses}>
                  Map author handle
                  <input
                    className={fieldClasses}
                    value={author}
                    onChange={(event) => setAuthor(event.target.value)}
                    required
                  />
                </label>
                <label className={fieldLabelClasses}>
                  Player handle
                  <input
                    className={fieldClasses}
                    value={player}
                    onChange={(event) => setPlayer(event.target.value)}
                    required
                  />
                </label>
              </>
            )}
          </fieldset>
          {error && (
            <p
              className={`${bodyTextClasses} text-[color:light-dark(#92233c,#ffacac)]`}
              role="alert"
            >
              {error}
            </p>
          )}
          <div className={actionsClasses}>
            <button
              className={buttonClasses()}
              type="button"
              disabled={working}
              onClick={dismiss}
            >
              Cancel
            </button>
            {prepareBackup && (
              <button
                className={buttonClasses()}
                type="button"
                disabled={working}
                onClick={() => void prepareCopy(prepareBackup)}
              >
                Prepare opening backup
              </button>
            )}
            <button
              className={buttonClasses("primary")}
              type="submit"
              disabled={working}
            >
              {working ? "Preparing…" : "Prepare download"}
            </button>
          </div>
        </form>
      )}
    </WebDialog>
  );
}

export function SwitchBankDialog({
  name,
  save,
  discard,
  cancel,
}: {
  name: string;
  save: () => void;
  discard: () => void;
  cancel: () => void;
}) {
  return (
    <WebDialog title="Unsaved bank changes" close={cancel}>
      <p className={bodyTextClasses}>
        Save a copy of your changes or discard them before opening {name}.
      </p>
      <div className={actionsClasses}>
        <button className={buttonClasses()} type="button" onClick={cancel}>
          Cancel
        </button>
        <button className={buttonClasses()} type="button" onClick={discard}>
          Discard and open
        </button>
        <button
          className={buttonClasses("primary")}
          type="button"
          onClick={save}
        >
          Save a copy…
        </button>
      </div>
    </WebDialog>
  );
}

export function ClipboardDialog({
  xml,
  close,
}: {
  xml: string;
  close: () => void;
}) {
  const [message, setMessage] = useState("Select and copy the prepared XML.");
  const clipboard = navigator.clipboard;

  function copy() {
    if (!clipboard) return;
    void clipboard.writeText(xml).then(
      () => {
        setMessage("Bank XML copied.");
      },
      () => {
        setMessage("Clipboard access was denied. Select and copy the XML.");
      },
    );
  }

  return (
    <WebDialog title="Copy bank XML" close={close}>
      <p className={bodyTextClasses} role="status">
        {message}
      </p>
      <textarea
        aria-label="Prepared bank XML"
        className={`${fieldClasses} min-h-40 [font-family:monospace]`}
        readOnly
        value={xml}
        onFocus={(event) => event.currentTarget.select()}
      />
      <div className={actionsClasses}>
        <button className={buttonClasses()} type="button" onClick={close}>
          Close
        </button>
        {typeof clipboard?.writeText === "function" && (
          <button
            className={buttonClasses("primary")}
            type="button"
            onClick={copy}
          >
            Copy XML
          </button>
        )}
      </div>
    </WebDialog>
  );
}

function WebDialog({
  title,
  close,
  busy,
  children,
}: {
  title: string;
  close: () => void;
  busy?: boolean;
  children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
    return () => element?.close();
  }, []);

  return (
    <dialog
      ref={dialog}
      aria-labelledby={titleId}
      className="m-auto w-[min(28rem,calc(100vw-2rem))] rounded-lg border border-line bg-surface p-4 text-ink backdrop:bg-[#1118]"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) close();
      }}
    >
      <h2 className="mb-4 font-semibold text-[1.125rem]" id={titleId}>
        {title}
      </h2>
      {children}
    </dialog>
  );
}
