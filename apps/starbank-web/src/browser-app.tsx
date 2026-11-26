import { EngineTransportError } from "@starbank/engine-client";
import type {
  EngineProfile,
  EngineSigningContext,
} from "@starbank/engine-client/protocol";
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
  preloadEditorTools,
  preloadSettingsPage,
} from "@starbank/ui";
import { useEffect, useRef, useState } from "react";
import {
  type BankEditCommand,
  type BrowserBankSession,
  createBankSession,
  encodeBankXml,
  readPreparedXml,
} from "./bank-session";
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
  const [document, setDocument] = useState<BankEditorDocument>();
  const [profile, setProfile] = useState<EngineProfile>();
  const [status, setStatus] = useState("Loading the bank engine…");
  const [ready, setReady] = useState(false);
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

  function activeDocument(target: EditorTarget): BankEditorDocument {
    if (document?.id !== target.id || document.revision !== target.revision) {
      throw new Error("The bank changed before the action could start.");
    }
    return document;
  }

  const controller: BankEditorController = {
    document,
    status,
    onDraftChange: setHasDraft,
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
          setProfile(undefined);
          return { ok: true };
        }
      : undefined,
    onEditValues: (target, edits) =>
      apply(target, { kind: "set_values", payload: { edits } }),
    onStructure: (target, change) =>
      apply(target, { kind: "structure", payload: change }),
    onRevert: (target, selection) =>
      apply(target, { kind: "revert", payload: selection }),
    onDiff: async (target) => {
      activeDocument(target);
      const diff = await bankSession().diff(target);
      return { ok: true, diff };
    },
    onReplaceXml: async (target, xml) =>
      publish(
        await bankSession().replaceXml(
          activeDocument(target).name,
          target,
          xml,
        ),
      ),
    profile: {
      current: profile ?? { status: "unloaded" },
      requireAuthor: true,
      onLoad: async (target, file, author) => {
        const { name } = activeDocument(target);
        setProfile(await bankSession().loadProfile(target, name, file, author));
        return { ok: true };
      },
      onClear: async (target) => {
        activeDocument(target);
        setProfile(await bankSession().clearProfile(target));
        return { ok: true };
      },
      onPreview: (target, { profile_id, field_id, input }) => {
        activeDocument(target);
        return bankSession().previewProfile(target, {
          profile_id,
          field_id,
          input,
        });
      },
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

  async function publish(next: BankEditorDocument) {
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

  async function apply(target: EditorTarget, command: BankEditCommand) {
    return publish(
      await bankSession().edit(activeDocument(target).name, target, command),
    );
  }

  function navigate(kind: "undo" | "redo" | "reset"): EditorAction {
    return (target) => apply(target, { kind });
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
        download: prepareBankDownload(encodeBankXml(document.xml), name),
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
      />
      {showExport && document && (
        <ExportDialog
          name={document.name}
          initialSign={preferences.settings.autoResignOnSave}
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

function isUnavailableEngine(error: unknown): boolean {
  if (!(error instanceof EngineTransportError)) return false;
  return error.code !== "busy" && error.code !== "limit";
}
