import { useEffect, useRef, useState } from "react";
import {
  AppShell,
  type BankEditorController,
  type EditorAction,
  type EditorActionResult,
  type EditorTarget,
  preloadEditorTools,
  preloadSettingsPage,
  type SettingsPlatformInfo,
} from "starbank-ui";
import {
  decodeBankFileXml,
  encodeBankFileXml,
} from "starbank-ui/bank-encoding";
import {
  prepareBankDownload,
  prepareOpeningBankBackup,
  writeBankClipboard,
} from "starbank-ui/browser";
import { EngineTransportError } from "starbank-ui/engine";
import {
  type EngineProfile,
  type EngineSigningContext,
  type EngineTarget,
  revisionAtLeast,
  SnapshotRefreshRequired,
} from "starbank-ui/engine/protocol";
import {
  type BankEditCommand,
  type BrowserBankDocument,
  type BrowserBankSession,
  createBankSession,
} from "./bank-session";
import { LocalBankLibrary } from "./local-bank-library";
import { useBrowserSettings } from "./use-browser-settings";
import { ClipboardDialog, ExportDialog, SwitchBankDialog } from "./web-dialogs";

declare const __STARBANK_PLATFORM_INFO__: SettingsPlatformInfo;

export function StarBankWeb() {
  const preferences = useBrowserSettings();

  useEffect(() => {
    const attributes = globalThis.document.documentElement.dataset;
    attributes.starbankTheme = preferences.settings.theme;
    attributes.starbankTextSize = preferences.settings.textSize;
  }, [preferences.settings.theme, preferences.settings.textSize]);
  const session = useRef<BrowserBankSession>(null);
  const actionInProgress = useRef(false);
  const validationInProgress = useRef<Promise<unknown>>(null);
  const currentDocument = useRef<BrowserBankDocument>(null);
  const [document, setDocument] = useState<BrowserBankDocument>();
  const [refreshRequest, setRefreshRequest] = useState<{
    displayed: EditorTarget;
    observed: EngineTarget | null;
  }>();
  const pendingRefresh =
    refreshRequest?.displayed.id === document?.id &&
    refreshRequest?.displayed.revision === document?.revision
      ? refreshRequest
      : undefined;
  const refreshTarget = pendingRefresh?.observed;
  const snapshotUnavailable = refreshTarget === null;
  const [openedFile, setOpenedFile] = useState<File>();
  const [busy, setBusy] = useState(false);
  const [profile, setProfile] = useState<EngineProfile>();
  const [status, setStatus] = useState("Loading the bank engine…");
  const [maxBankBytes, setMaxBankBytes] = useState<number>();
  const [hasDraft, setHasDraft] = useState(false);
  const [exportTarget, setExportTarget] = useState<EditorTarget>();
  const showExport =
    (pendingRefresh === undefined || snapshotUnavailable) &&
    exportTarget?.id === document?.id &&
    exportTarget?.revision === document?.revision &&
    exportTarget !== undefined;
  const [copyPreview, setCopyPreview] = useState<string>();
  const [pendingOpen, setPendingOpen] = useState<{
    save: boolean;
    nextName: string;
    resolve: (proceed: boolean) => void;
  }>();

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
    const document = currentDocument.current;
    if (document?.id !== target.id || document.revision !== target.revision) {
      throw new Error("The bank changed before the action could start.");
    }
    return document;
  }

  function publishDocument(next: BrowserBankDocument) {
    currentDocument.current = next;
    setDocument(next);
    setRefreshRequest(undefined);
  }

  function rememberRefresh(error: unknown): unknown {
    const displayed = currentDocument.current;
    if (!displayed) return error;
    const scoped =
      error instanceof SnapshotRefreshRequired &&
      error.target.document_id === displayed.id &&
      revisionAtLeast(error.target.expected_revision, displayed.revision)
        ? error
        : undefined;
    const engineStopped =
      (scoped && isUnavailableEngine(scoped.cause)) ||
      (pendingRefresh !== undefined && isUnavailableEngine(error));
    if (scoped || engineStopped) {
      setExportTarget(undefined);
      setRefreshRequest({
        displayed: { id: displayed.id, revision: displayed.revision },
        observed: engineStopped ? null : (scoped?.target ?? null),
      });
    }
    return engineStopped
      ? new Error("The bank engine stopped. Any draft is kept for review.")
      : error;
  }

  async function runBankAction<Result>(
    action: () => Promise<Result>,
  ): Promise<Result> {
    if (actionInProgress.current)
      throw new Error("Wait for the current bank action to finish.");
    actionInProgress.current = true;
    setBusy(true);
    try {
      // A discarded draft can still have a read-only command in the worker.
      const validation = validationInProgress.current;
      if (validation) await validation.catch(() => {});
      return await action();
    } catch (error) {
      throw rememberRefresh(error);
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
      if (refreshTarget) throw new SnapshotRefreshRequired(refreshTarget);
      const discardCurrent = document?.dirty ?? false;
      if (discardCurrent) {
        const proceed = await new Promise<boolean>((resolve) => {
          setPendingOpen({ nextName: file.name, save: false, resolve });
        });
        if (!proceed)
          return { ok: false, message: "The current bank is still open." };
      }
      publishDocument(await bankSession().open(file, discardCurrent));
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
    snapshotUnavailable,
    onRefreshSnapshot: refreshTarget
      ? async (target) => {
          const result = await updateDocument(target, ({ name }) =>
            bankSession().refresh(name, refreshTarget),
          );
          return { ...result, message: result.message ?? "Editor refreshed." };
        }
      : undefined,
    onRecoveryCopy: snapshotUnavailable
      ? async (target) => {
          activeDocument(target);
          setExportTarget(target);
          return { ok: true };
        }
      : undefined,
    onOpenFile: maxBankBytes === undefined ? undefined : openBank,
    onValidateValues: async (target, edits) => {
      if (actionInProgress.current || validationInProgress.current)
        throw new Error("Wait for the current bank action to finish.");
      activeDocument(target);
      const validation = bankSession().validateValues(target, edits);
      validationInProgress.current = validation;
      try {
        return await validation;
      } catch (error) {
        throw rememberRefresh(error);
      } finally {
        validationInProgress.current = null;
      }
    },
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
      updateDocument(target, ({ name }) =>
        bankSession().replaceXml(name, target, xml),
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
      setExportTarget(target);
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
    let message: string | undefined;
    if (profile !== undefined && profile.status !== "unloaded") {
      try {
        setProfile(await bankSession().profile(next));
      } catch (error) {
        if (error instanceof SnapshotRefreshRequired) throw error;
        setProfile(undefined);
        message = "Bank updated. Reload the profile to refresh Simple View.";
      }
    }
    publishDocument(next);
    return {
      ok: true,
      target: { id: next.id, revision: next.revision },
      message,
    } as const;
  }

  function apply(target: EditorTarget, command: BankEditCommand) {
    return updateDocument(target, ({ name }) =>
      bankSession().edit(name, target, command),
    );
  }

  function updateDocument(
    target: EditorTarget,
    load: (document: BrowserBankDocument) => Promise<BrowserBankDocument>,
  ) {
    return runBankAction(async () => {
      return publish(await load(activeDocument(target)));
    });
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

  async function exportCopy(
    target: EditorTarget,
    name: string,
    context?: EngineSigningContext,
  ) {
    const document = activeDocument(target);
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
      return recoveryCopy(document, name, context);
    }
  }

  function finishSwitch(proceed: boolean) {
    setPendingOpen(undefined);
    pendingOpen?.resolve(proceed);
  }

  return (
    <>
      <AppShell
        platformInfo={__STARBANK_PLATFORM_INFO__}
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
                busy ||
                hasDraft ||
                pendingRefresh !== undefined ||
                showExport ||
                copyPreview !== undefined
              }
              open={openBank}
            />
          )
        }
      />
      {showExport && document && !pendingOpen && (
        <ExportDialog
          name={document.name}
          signingAvailable={!snapshotUnavailable}
          initialSign={
            !snapshotUnavailable && preferences.settings.autoResignOnSave
          }
          prepare={(name, context) =>
            runBankAction(async () =>
              snapshotUnavailable
                ? recoveryCopy(activeDocument(exportTarget), name, context)
                : exportCopy(exportTarget, name, context),
            )
          }
          prepareBackup={
            snapshotUnavailable
              ? undefined
              : () => runBankAction(() => exportOpeningBackup(exportTarget))
          }
          dismiss={() => setExportTarget(undefined)}
        />
      )}
      {pendingOpen &&
        document &&
        (pendingOpen.save ? (
          <ExportDialog
            name={document.name.replace(/\.xml$/i, ".SC2Bank")}
            initialSign={preferences.settings.autoResignOnSave}
            prepare={(name, context) =>
              exportCopy(document, name, context).catch((error) => {
                throw rememberRefresh(error);
              })
            }
            continueAfterDownload={() => finishSwitch(true)}
            dismiss={() => finishSwitch(false)}
          />
        ) : (
          <SwitchBankDialog
            name={pendingOpen.nextName}
            save={() => setPendingOpen({ ...pendingOpen, save: true })}
            discard={() => finishSwitch(true)}
            cancel={() => finishSwitch(false)}
          />
        ))}
      {copyPreview !== undefined && (
        <ClipboardDialog
          xml={copyPreview}
          close={() => setCopyPreview(undefined)}
        />
      )}
    </>
  );
}

function recoveryCopy(
  document: BrowserBankDocument,
  name: string,
  context?: EngineSigningContext,
) {
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
      "The bank engine stopped. This recovery copy contains the last accepted revision. It excludes drafts and revisions the editor could not display. Its signature has not been re-verified.",
  };
}

function isUnavailableEngine(error: unknown): boolean {
  if (!(error instanceof EngineTransportError)) return false;
  return error.code !== "busy" && error.code !== "limit";
}
