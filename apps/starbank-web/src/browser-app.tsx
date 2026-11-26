import { useEffect, useRef, useState } from "react";
import { EngineTransportError } from "starbank-engine-client";
import type {
  EngineProfile,
  EngineSigningContext,
} from "starbank-engine-client/protocol";
import {
  decodeBankFileXml,
  encodeBankFileXml,
} from "starbank-platform/bank-encoding";
import {
  prepareBankDownload,
  prepareOpeningBankBackup,
  writeBankClipboard,
} from "starbank-platform/browser";
import {
  AppShell,
  type BankEditorController,
  type EditorAction,
  type EditorActionResult,
  type EditorTarget,
  preloadEditorTools,
  preloadSettingsPage,
} from "starbank-ui";
import {
  type BankEditCommand,
  type BrowserBankDocument,
  type BrowserBankSession,
  createBankSession,
} from "./bank-session";
import { LocalBankLibrary } from "./local-bank-library";
import { useBrowserSettings } from "./use-browser-settings";
import { ClipboardDialog, ExportDialog } from "./web-dialogs";
import "./web-dialogs.css";

export function StarBankWeb() {
  const preferences = useBrowserSettings();

  useEffect(() => {
    const attributes = globalThis.document.documentElement.dataset;
    attributes.starbankTheme = preferences.settings.theme;
    attributes.starbankTextSize = preferences.settings.textSize;
  }, [preferences.settings.theme, preferences.settings.textSize]);
  const session = useRef<BrowserBankSession>(null);
  const actionInProgress = useRef(false);
  const [document, setDocument] = useState<BrowserBankDocument>();
  const [openedFile, setOpenedFile] = useState<File>();
  const [busy, setBusy] = useState(false);
  const [profile, setProfile] = useState<EngineProfile>();
  const [status, setStatus] = useState("Loading the bank engine…");
  const [maxBankBytes, setMaxBankBytes] = useState<number>();
  const [hasDraft, setHasDraft] = useState(false);
  const [showExport, setShowExport] = useState(false);
  const [copyPreview, setCopyPreview] = useState<string>();

  useEffect(() => {
    let mounted = true;
    try {
      session.current = createBankSession();
      void Promise.all([
        session.current.ready(),
        preloadEditorTools(),
        preloadSettingsPage(),
      ]).then(
        ([engine]) => {
          if (mounted) {
            setMaxBankBytes(engine.maxBankBytes);
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
    if (!document?.dirty && !hasDraft) return;
    const confirmNavigation = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", confirmNavigation);
    return () => window.removeEventListener("beforeunload", confirmNavigation);
  }, [document?.dirty, hasDraft]);

  function bankSession(): BrowserBankSession {
    if (!session.current) throw new Error("The bank engine is unavailable.");
    return session.current;
  }

  function activeDocument(target: EditorTarget): BrowserBankDocument {
    if (document?.id !== target.id || document.revision !== target.revision) {
      throw new Error("The bank changed before the action could start.");
    }
    return document;
  }

  async function runBankAction<Result>(
    action: () => Promise<Result>,
  ): Promise<Result> {
    if (actionInProgress.current)
      throw new Error("Wait for the current bank action to finish.");
    actionInProgress.current = true;
    setBusy(true);
    try {
      return await action();
    } finally {
      actionInProgress.current = false;
      setBusy(false);
    }
  }

  function openBank(file: File): Promise<EditorActionResult> {
    return runBankAction(async () => {
      if (hasDraft) {
        return {
          ok: false,
          message: "Apply or discard your draft before opening another bank.",
        };
      }
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
      setOpenedFile(file);
      setProfile(undefined);
      return { ok: true };
    });
  }

  const controller: BankEditorController = {
    document,
    busy,
    status,
    onDraftChange: setHasDraft,
    onOpenFile: maxBankBytes === undefined ? undefined : openBank,
    onEditValues: (target, edits) =>
      apply(target, { kind: "set_values", payload: { edits } }),
    onStructure: (target, change) =>
      apply(target, { kind: "structure", payload: change }),
    onRevert: (target, selection) =>
      apply(target, { kind: "revert", payload: selection }),
    onDiff: (target) =>
      runBankAction(async () => {
        activeDocument(target);
        const diff = await bankSession().diff(target);
        return { ok: true, diff };
      }),
    onReplaceXml: (target, xml) =>
      runBankAction(async () =>
        publish(
          await bankSession().replaceXml(
            activeDocument(target).name,
            target,
            xml,
          ),
        ),
      ),
    profile: {
      current: profile ?? { status: "unloaded" },
      requireAuthor: true,
      onLoad: (target, file, author) =>
        runBankAction(async () => {
          const { name } = activeDocument(target);
          setProfile(
            await bankSession().loadProfile(target, name, file, author),
          );
          return { ok: true };
        }),
      onClear: (target) =>
        runBankAction(async () => {
          activeDocument(target);
          setProfile(await bankSession().clearProfile(target));
          return { ok: true };
        }),
      onPreview: (target, { profile_id, field_id, input }) =>
        runBankAction(async () => {
          activeDocument(target);
          return bankSession().previewProfile(target, {
            profile_id,
            field_id,
            input,
          });
        }),
      onApply: (target, previewId) =>
        apply(target, {
          kind: "profile_apply",
          payload: { preview_id: previewId },
        }),
    },
    onUndo: navigate("undo"),
    onRedo: navigate("redo"),
    onReset: navigate("reset"),
    onSaveAs: async (target) => {
      activeDocument(target);
      setShowExport(true);
      return { ok: true };
    },
    onCopy: (target) =>
      runBankAction(async () => {
        activeDocument(target);
        const text = bankSession()
          .prepare(target)
          .then(({ bytes }) => decodeBankFileXml(bytes));
        try {
          await writeBankClipboard(text);
          return { ok: true, message: "Bank XML copied." };
        } catch {
          setCopyPreview(await text);
          return { ok: true };
        }
      }),
  };

  async function publish(next: BrowserBankDocument) {
    if (profile !== undefined && profile.status !== "unloaded") {
      try {
        setProfile(await bankSession().profile(next));
      } catch {
        setProfile(undefined);
        setDocument(next);
        return {
          ok: true,
          message: "Bank updated. Reload the profile to refresh Simple View.",
        } as const;
      }
    }
    setDocument(next);
    return { ok: true } as const;
  }

  function apply(target: EditorTarget, command: BankEditCommand) {
    return runBankAction(async () =>
      publish(
        await bankSession().edit(activeDocument(target).name, target, command),
      ),
    );
  }

  function navigate(kind: "undo" | "redo" | "reset"): EditorAction {
    return (target) => apply(target, { kind });
  }

  async function exportOpeningBackup(target: EditorTarget) {
    const bank = activeDocument(target);
    const bytes = await bankSession().openingBytes(target);
    return {
      download: prepareOpeningBankBackup(bytes, bank.name),
      message:
        "This backup contains the exact file bytes from when the bank was opened.",
    };
  }

  async function exportCopy(name: string, context?: EngineSigningContext) {
    if (!document) throw new Error("Open a bank to prepare a download.");
    try {
      const { bytes, signatureStatus } = await bankSession().prepare(
        document,
        context,
      );
      return {
        download: prepareBankDownload(bytes, name),
        message:
          signatureStatus === "valid"
            ? "Signature verified. Keep this filename when using the bank."
            : signatureStatus === "unsigned"
              ? undefined
              : "This copy contains a signature that could not be verified.",
      };
    } catch (error) {
      if (!isUnavailableEngine(error)) throw error;
      if (context) {
        throw new Error(
          "The bank engine stopped. Clear Re-sign this copy to prepare a recovery download.",
        );
      }
      return {
        download: prepareBankDownload(
          encodeBankFileXml(document.xml, document.sourceEncoding),
          name,
        ),
        message:
          "The bank engine stopped. This recovery copy contains the last accepted revision. Its signature has not been re-verified.",
      };
    }
  }

  return (
    <>
      <AppShell
        name="StarBank Web"
        editor={controller}
        preferences={preferences}
        workspaceAccessory={
          maxBankBytes !== undefined && (
            <LocalBankLibrary
              maxBankBytes={maxBankBytes}
              openedFile={openedFile}
              hasDraft={hasDraft}
              switchingBlocked={
                busy || hasDraft || showExport || copyPreview !== undefined
              }
              open={openBank}
            />
          )
        }
      />
      {showExport && document && (
        <ExportDialog
          name={document.name}
          initialSign={preferences.settings.autoResignOnSave}
          prepare={(name, context) =>
            runBankAction(() => exportCopy(name, context))
          }
          prepareBackup={() =>
            runBankAction(() => exportOpeningBackup(document))
          }
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

function isUnavailableEngine(error: unknown): boolean {
  if (!(error instanceof EngineTransportError)) return false;
  return error.code !== "busy" && error.code !== "limit";
}
