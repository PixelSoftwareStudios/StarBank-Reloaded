import type { EngineSigningContext } from "@starbank/engine-client/protocol";
import {
  prepareBankDownload,
  writeBankClipboard,
} from "@starbank/platform/browser";
import {
  AppShell,
  type BankEditorController,
  type BankEditorDocument,
  type EditorAction,
  type EditorTarget,
  preloadRawEditor,
} from "@starbank/ui";
import { useEffect, useRef, useState } from "react";
import {
  type BrowserBankSession,
  createBankSession,
  readPreparedXml,
} from "./bank-session";
import { ClipboardDialog, ExportDialog } from "./web-dialogs";
import "./web-dialogs.css";

export function StarBankWeb() {
  const session = useRef<BrowserBankSession>(null);
  const [document, setDocument] = useState<BankEditorDocument>();
  const [status, setStatus] = useState("Loading the bank engine…");
  const [ready, setReady] = useState(false);
  const [showExport, setShowExport] = useState(false);
  const [copyPreview, setCopyPreview] = useState<string>();

  useEffect(() => {
    let mounted = true;
    try {
      session.current = createBankSession();
      void Promise.all([session.current.ready(), preloadRawEditor()]).then(
        () => {
          if (mounted) {
            setReady(true);
            setStatus("");
          }
        },
        () => {
          if (mounted)
            setStatus(
              "The bank engine could not start. Reload StarBank Web to try again.",
            );
        },
      );
    } catch {
      setStatus("This browser could not start the bank engine.");
    }
    return () => {
      mounted = false;
      session.current?.close();
    };
  }, []);

  useEffect(() => {
    if (!document?.dirty) return;
    const confirmNavigation = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", confirmNavigation);
    return () => window.removeEventListener("beforeunload", confirmNavigation);
  }, [document?.dirty]);

  function bankSession(): BrowserBankSession {
    if (!session.current) throw new Error("The bank engine is unavailable.");
    return session.current;
  }

  function activeDocument(target: EditorTarget): BankEditorDocument {
    if (document?.id !== target.id || document.revision !== target.revision) {
      throw new Error("The bank changed before the action could start.");
    }
    return document;
  }

  const controller: BankEditorController = {
    document,
    status,
    onOpenFile: ready
      ? async (file) => {
          const discardCurrent = document?.dirty ?? false;
          if (
            discardCurrent &&
            !window.confirm(
              "Discard changes in the current bank and open another bank?",
            )
          ) {
            return { ok: false, message: "The current bank is still open." };
          }
          setDocument(await bankSession().open(file, discardCurrent));
          return { ok: true };
        }
      : undefined,
    onEditValues: async (target, edits) => {
      setDocument(
        await bankSession().edit(activeDocument(target).name, target, {
          kind: "set_values",
          payload: { edits },
        }),
      );
      return { ok: true };
    },
    onReplaceXml: async (target, xml) => {
      setDocument(
        await bankSession().replaceXml(
          activeDocument(target).name,
          target,
          xml,
        ),
      );
      return { ok: true };
    },
    onUndo: navigate("undo"),
    onRedo: navigate("redo"),
    onReset: navigate("reset"),
    onSaveAs: async (target) => {
      activeDocument(target);
      setShowExport(true);
      return { ok: true };
    },
    onCopy: async (target) => {
      activeDocument(target);
      const text = bankSession()
        .prepare(target)
        .then(({ bytes }) => readPreparedXml(bytes));
      try {
        await writeBankClipboard(text);
        return { ok: true, message: "Bank XML copied." };
      } catch {
        setCopyPreview(await text);
        return { ok: true };
      }
    },
  };

  function navigate(kind: "undo" | "redo" | "reset"): EditorAction {
    return async (target) => {
      setDocument(
        await bankSession().edit(activeDocument(target).name, target, { kind }),
      );
      return { ok: true };
    };
  }

  async function exportCopy(name: string, context?: EngineSigningContext) {
    if (!document) throw new Error("Open a bank to prepare a download.");
    const { bytes, signatureStatus } = await bankSession().prepare(
      document,
      context,
    );
    return {
      download: prepareBankDownload(bytes, name),
      signatureMessage:
        signatureStatus === "valid"
          ? "Signature verified. Keep this filename when using the bank."
          : signatureStatus === "unsigned"
            ? undefined
            : "This copy contains a signature that could not be verified.",
    };
  }

  return (
    <>
      <AppShell name="StarBank Web" editor={controller} />
      {showExport && document && (
        <ExportDialog
          name={document.name}
          prepare={exportCopy}
          dismiss={() => setShowExport(false)}
        />
      )}
      {copyPreview !== undefined && (
        <ClipboardDialog
          xml={copyPreview}
          close={() => setCopyPreview(undefined)}
        />
      )}
    </>
  );
}
