import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import {
  bankFilenameStem,
  type EngineSigningContext,
} from "starbank-engine-client/protocol";
import type { BankDownload } from "starbank-platform/browser";

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
          {prepared.message && <p role="status">{prepared.message}</p>}
          <div className="web-dialog-actions">
            <button
              className="editor-button"
              type="button"
              onClick={() => setPrepared(undefined)}
            >
              Back
            </button>
            <button className="editor-button" type="button" onClick={dismiss}>
              {continueAfterDownload ? "Cancel" : "Close"}
            </button>
            <a
              ref={downloadLink}
              className="editor-button editor-primary"
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
                className="editor-button editor-primary"
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
          <fieldset disabled={working} className="web-dialog-fields">
            <label>
              Filename
              <input
                value={filename}
                onChange={(event) => setFilename(event.target.value)}
                required
              />
            </label>
            <label className="web-dialog-check">
              <input
                type="checkbox"
                checked={sign}
                onChange={(event) => setSign(event.target.checked)}
              />
              Re-sign this copy
            </label>
            {sign && (
              <>
                <label>
                  Map author handle
                  <input
                    value={author}
                    onChange={(event) => setAuthor(event.target.value)}
                    required
                  />
                </label>
                <label>
                  Player handle
                  <input
                    value={player}
                    onChange={(event) => setPlayer(event.target.value)}
                    required
                  />
                </label>
              </>
            )}
          </fieldset>
          {error && <p role="alert">{error}</p>}
          <div className="web-dialog-actions">
            <button
              className="editor-button"
              type="button"
              disabled={working}
              onClick={dismiss}
            >
              Cancel
            </button>
            {prepareBackup && (
              <button
                className="editor-button"
                type="button"
                disabled={working}
                onClick={() => void prepareCopy(prepareBackup)}
              >
                Prepare opening backup
              </button>
            )}
            <button
              className="editor-button editor-primary"
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
      <p>Save a copy of your changes or discard them before opening {name}.</p>
      <div className="web-dialog-actions">
        <button className="editor-button" type="button" onClick={cancel}>
          Cancel
        </button>
        <button className="editor-button" type="button" onClick={discard}>
          Discard and open
        </button>
        <button
          className="editor-button editor-primary"
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
      <p role="status">{message}</p>
      <textarea
        aria-label="Prepared bank XML"
        readOnly
        value={xml}
        onFocus={(event) => event.currentTarget.select()}
      />
      <div className="web-dialog-actions">
        <button className="editor-button" type="button" onClick={close}>
          Close
        </button>
        {typeof clipboard?.writeText === "function" && (
          <button
            className="editor-button editor-primary"
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
      className="web-dialog"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) close();
      }}
    >
      <h2 id={titleId}>{title}</h2>
      {children}
    </dialog>
  );
}
