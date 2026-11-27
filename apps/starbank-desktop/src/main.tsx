import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import type {
  BankEditorController,
  BankEditorDocument,
  EditorActionResult,
  EditorMutationResult,
  EditorRow,
  EditorTarget,
  SettingsPlatformInfo,
} from "starbank-ui";
import { AppShell } from "starbank-ui";
import { decodeBankFileXml } from "starbank-ui/bank-encoding";
import type {
  EngineCommand,
  EngineDiff,
  EngineProfile,
  EngineReply,
  EngineSnapshot,
  EngineTarget,
} from "starbank-ui/engine/protocol";
import {
  decodeEngineReply,
  engineReplyError,
  engineRequest,
  profileLoadForFile,
  revisionAtLeast,
  SnapshotRefreshRequired,
} from "starbank-ui/engine/protocol";
import {
  type BackupAction,
  BackupPicker,
  type BackupPrompt,
} from "./backup-picker";
import {
  BackupPreferencesFields,
  useBackupPreferences,
} from "./backup-preferences";
import { BackupRetentionReview } from "./backup-retention";
import { useActiveBackupStatus } from "./backup-status";
import { BankConflictDialog, type BankConflictPrompt } from "./bank-conflict";
import { BatchBackupTools } from "./batch-backup-ui";
import { resignExternalBank } from "./external-sign";
import {
  type ManualSigningPrompt,
  ManualSigningSheet,
} from "./manual-signing-sheet";
import { OpenRequests, openRequestedBank } from "./open-requests";
import { resignOpenBank } from "./resign-bank";
import type { ActiveBankTarget, FileDialogReply } from "./transport";
import {
  canExportBank,
  canRevealBank,
  compareBankConflict,
  confirmBankNavigation,
  createBatchBackup,
  decodeBankXml,
  dispatchBank,
  encodeBankXml,
  exportActiveBackup,
  exportBankCopy,
  listActiveBackups,
  pickBank,
  reopenBankConflict,
  restoreActiveBank,
  revealActiveBank,
  reviewActiveBackup,
  saveActiveBank,
  saveBankAs,
} from "./transport";
import { useDesktopSettings } from "./use-desktop-settings";
import { openInventoryBank, useInventory } from "./use-inventory";
import { useNativeCloseGuard } from "./use-native-close-guard";
import "starbank-ui/styles.css";

declare const __STARBANK_PLATFORM_INFO__: SettingsPlatformInfo;

const APP_NAME = "StarBank Reloaded";

interface DesktopDocument {
  projection: BankEditorDocument;
  canUndo: boolean;
  canRedo: boolean;
}

type OpenBankSource = (control: string) => Promise<FileDialogReply | null>;

interface AcceptedOpen {
  readonly previous: DesktopDocument | null;
  readonly target: EngineTarget;
  readonly name: string;
  readonly breadcrumbs?: readonly string[];
  readonly canOverwrite: boolean;
  readonly bankId?: string;
}

function DesktopApp() {
  const currentDocument = useRef<DesktopDocument>(null);
  const acceptedOpen = useRef<AcceptedOpen>(null);
  const [openRecovery, setOpenRecovery] = useState<AcceptedOpen>();
  const [document, setDocument] = useState<DesktopDocument>();
  const [refreshRequest, setRefreshRequest] = useState<{
    displayed: EditorTarget;
    observed: EngineTarget;
  }>();
  const refreshTarget =
    refreshRequest?.displayed.id === document?.projection.id &&
    refreshRequest?.displayed.revision === document?.projection.revision
      ? refreshRequest?.observed
      : undefined;
  const [profile, setProfile] = useState<EngineProfile>();
  const [hasDraft, setHasDraft] = useState(false);
  const currentDraft = useRef(false);
  const [nativeNotice, setNativeNotice] = useState<string>();
  const [selectedBankId, setSelectedBankId] = useState<string>();
  const [backupGeneration, setBackupGeneration] = useState(0);
  const [busy, setBusy] = useState(false);
  const [exportAvailable, setExportAvailable] = useState(false);
  const [revealAvailable, setRevealAvailable] = useState(false);
  const [overwriteAvailable, setOverwriteAvailable] = useState(false);
  const [backupPrompt, setBackupPrompt] = useState<BackupPrompt>();
  const [conflictPrompt, setConflictPrompt] = useState<BankConflictPrompt>();
  const [signingPrompt, setSigningPrompt] = useState<ManualSigningPrompt>();
  const inProgress = useRef(false);
  const nextRequest = useRef(0);
  const preferences = useDesktopSettings();
  const backupPreferences = useBackupPreferences(exportAvailable);
  const { theme, textSize } = preferences.settings;
  const backupStatus = useActiveBackupStatus(
    document?.projection,
    backupGeneration,
  );

  const closeGuard = useNativeCloseGuard({
    busy,
    onDraft: (present) => {
      currentDraft.current = present;
      setHasDraft(present);
    },
    onNotice: setNativeNotice,
    save:
      document &&
      !refreshTarget &&
      !openRecovery &&
      (overwriteAvailable || exportAvailable)
        ? async () => {
            const result = await save(
              document.projection,
              overwriteAvailable ? "overwrite" : "new",
            );
            if (result.message) setNativeNotice(result.message);
            return result.ok && result.message !== undefined;
          }
        : undefined,
  });

  useEffect(() => {
    window.document.documentElement.dataset.starbankTheme = theme;
    window.document.documentElement.dataset.starbankTextSize = textSize;
  }, [theme, textSize]);

  useEffect(() => {
    void canExportBank().then(setExportAvailable, () => {});
    void canRevealBank().then(setRevealAvailable, () => {});
  }, []);

  function requestId() {
    nextRequest.current += 1;
    return `desktop-${nextRequest.current}`;
  }

  async function exclusive<Result>(
    run: () => Promise<Result>,
  ): Promise<Result> {
    if (inProgress.current) {
      throw new Error("A bank operation is already in progress.");
    }
    inProgress.current = true;
    setNativeNotice(undefined);
    setBusy(true);
    try {
      return await run();
    } catch (error) {
      if (
        error === "active_write_uncertain" ||
        error === "saved_needs_reopen"
      ) {
        setOverwriteAvailable(false);
      }
      throw error;
    } finally {
      inProgress.current = false;
      setBusy(false);
    }
  }

  async function action<Result extends EditorActionResult>(
    run: () => Promise<Result>,
  ): Promise<Result | Extract<EditorActionResult, { ok: false }>> {
    try {
      return await exclusive(run);
    } catch (error) {
      return { ok: false, message: messageFor(error) };
    }
  }

  async function dispatch(
    target: EditorTarget,
    command: EngineCommand,
    bytes?: Uint8Array,
  ) {
    const opening = acceptedOpen.current;
    if (
      opening &&
      (target.id !== opening.target.document_id ||
        target.revision !== opening.target.expected_revision ||
        (command.kind !== "snapshot" && command.kind !== "raw_text"))
    ) {
      throw new Error("Finish opening the accepted bank first.");
    }
    const id = requestId();
    const scope = {
      document_id: target.id,
      expected_revision: target.revision,
    };
    const control = engineRequest(id, command, scope);
    const nativeReply = await dispatchBank(JSON.stringify(control), bytes);
    const reply = decodeEngineReply(nativeReply.control, id);
    const error = engineReplyError(reply, scope);
    if (error) {
      if (error instanceof SnapshotRefreshRequired)
        rememberRefresh(error.target);
      throw error;
    }
    return { reply, bytes: nativeReply.bytes };
  }

  async function projectDocument(
    name: string,
    reply: EngineReply,
    breadcrumbs?: readonly string[],
  ): Promise<DesktopDocument> {
    const { target, snapshot } = requireSnapshot(reply);
    const raw = await dispatch(target, { kind: "raw_text" });
    if (
      raw.reply.result.kind !== "raw_text" ||
      raw.reply.document_id !== target.id ||
      raw.reply.revision !== target.revision ||
      raw.bytes === undefined
    ) {
      throw new Error("The bank engine did not return XML.");
    }
    return {
      projection: {
        ...target,
        name,
        breadcrumbs:
          breadcrumbs ??
          (document?.projection.id === target.id
            ? document.projection.breadcrumbs
            : undefined),
        dirty: snapshot.dirty,
        dirtySectionCount: snapshot.dirty_section_count,
        sections: displaySections(snapshot),
        signatureStatus: snapshot.signature_status,
        xml: decodeBankXml(raw.bytes),
      },
      canUndo: snapshot.can_undo,
      canRedo: snapshot.can_redo,
    };
  }

  async function adopt(
    name: string,
    reply: EngineReply,
    breadcrumbs?: readonly string[],
  ) {
    const next = await projectDocument(name, reply, breadcrumbs);
    const target = next.projection;
    if (document?.projection.id !== target.id) {
      setProfile(undefined);
    } else if (profile && profile.status !== "unloaded") {
      try {
        await updateProfile(target, { kind: "profile_snapshot" });
      } catch (error) {
        if (error instanceof SnapshotRefreshRequired) throw error;
        setProfile(undefined);
      }
    }
    publishDocument(next);
    return { id: target.id, revision: target.revision };
  }

  function publishDocument(next: DesktopDocument) {
    currentDocument.current = next;
    setDocument(next);
    setRefreshRequest(undefined);
  }

  async function updateProfile(
    target: EditorTarget,
    command: Extract<
      EngineCommand,
      {
        kind: "profile_snapshot" | "profile_load" | "profile_clear";
      }
    >,
    bytes?: Uint8Array,
  ) {
    const { reply } = await dispatch(target, command, bytes);
    if (
      reply.result.kind !== "profile" ||
      reply.document_id !== target.id ||
      reply.revision !== target.revision
    ) {
      throw new Error("The bank changed before its profile could be loaded.");
    }
    setProfile(reply.result.payload);
  }

  function update(
    target: EditorTarget,
    command: EngineCommand,
    bytes?: Uint8Array,
  ) {
    return updateDocument(
      target,
      async () => (await dispatch(target, command, bytes)).reply,
    );
  }

  function updateDocument(
    target: EditorTarget,
    load: () => Promise<EngineReply>,
    expectedRevision?: string,
  ) {
    return action<EditorMutationResult>(async () => {
      const { projection } = activeDocument(target);
      let accepted: EngineTarget | undefined;
      try {
        const reply = await load();
        if (
          reply.result.kind !== "snapshot" ||
          reply.document_id !== target.id ||
          reply.revision === null ||
          !revisionAtLeast(reply.revision, target.revision) ||
          (expectedRevision !== undefined &&
            reply.revision !== expectedRevision)
        ) {
          throw new Error(
            "The bank engine did not return the requested snapshot.",
          );
        }
        accepted = {
          document_id: reply.document_id,
          expected_revision: reply.revision,
        };
        const next = await adopt(projection.name, reply);
        return {
          ok: true,
          target: next,
          message:
            expectedRevision === undefined ? undefined : "Editor refreshed.",
        };
      } catch (error) {
        if (error instanceof SnapshotRefreshRequired) throw error;
        if (!accepted) throw error;
        rememberRefresh(accepted);
        throw new SnapshotRefreshRequired(accepted, error);
      }
    });
  }

  function rememberRefresh(observed: EngineTarget) {
    const displayed = currentDocument.current?.projection;
    if (
      displayed?.id === observed.document_id &&
      revisionAtLeast(observed.expected_revision, displayed.revision)
    ) {
      setRefreshRequest({
        displayed: { id: displayed.id, revision: displayed.revision },
        observed,
      });
    }
  }

  function activeDocument(target: EditorTarget): DesktopDocument {
    const document = currentDocument.current;
    if (
      document?.projection.id !== target.id ||
      document.projection.revision !== target.revision
    ) {
      throw new Error("The bank changed before the action could start.");
    }
    return document;
  }

  function activeTarget(target: EditorTarget): ActiveBankTarget {
    if (acceptedOpen.current) {
      throw new Error("Finish opening the accepted bank first.");
    }
    return {
      documentId: target.id,
      revision: target.revision,
      requestId: requestId(),
    };
  }

  function openFrom(source: OpenBankSource, bankId?: string) {
    return action(async () => {
      if (acceptedOpen.current) {
        throw new Error("Finish opening the accepted bank first.");
      }
      if (currentDraft.current) {
        return {
          ok: false,
          message: "Apply or discard the current draft first.",
        };
      }
      if (refreshTarget) throw new SnapshotRefreshRequired(refreshTarget);
      let discardRequestId: string | undefined;
      const current = document?.projection;
      if (current?.dirty) {
        const mode = overwriteAvailable
          ? "overwrite"
          : exportAvailable
            ? "new"
            : undefined;
        const target = activeTarget(current);
        const choice = await confirmBankNavigation(target, mode !== undefined);
        if (choice === "discard") discardRequestId = target.requestId;
        else if (choice === "save" && mode) {
          const saved = await saveCurrent(current, mode);
          if (!saved.ok || !saved.message) return saved;
        } else return { ok: true };
      }
      if (currentDraft.current) {
        throw new Error("Apply or discard the current draft first.");
      }
      const previous = currentDocument.current;
      const id = discardRequestId ?? requestId();
      const control = engineRequest(
        id,
        discardRequestId
          ? { kind: "open", payload: { discard_current: true } }
          : { kind: "open" },
      );
      const opened = await source(JSON.stringify(control));
      if (!opened) return { ok: true };
      const reply = decodeEngineReply(opened.control, id);
      const { target } = requireSnapshot(reply);
      if (target.id === previous?.projection.id) {
        throw new Error("The bank engine did not acknowledge a new document.");
      }
      const opening = {
        previous,
        target: { document_id: target.id, expected_revision: target.revision },
        name: opened.name,
        breadcrumbs: opened.breadcrumbs,
        canOverwrite: opened.can_overwrite === true,
        bankId,
      };
      acceptedOpen.current = opening;
      return displayOpening(opening, async () => reply);
    });
  }

  async function displayOpening(
    opening: AcceptedOpen,
    load: () => Promise<EngineReply>,
  ): Promise<EditorActionResult> {
    function stillOpening() {
      return (
        acceptedOpen.current === opening &&
        currentDocument.current === opening.previous
      );
    }

    try {
      if (!stillOpening()) throw new Error("The selected bank changed.");
      if (currentDraft.current) {
        throw new Error("Discard the previous bank draft before finishing.");
      }
      const reply = await load();
      const { target } = requireSnapshot(reply);
      if (
        target.id !== opening.target.document_id ||
        target.revision !== opening.target.expected_revision
      ) {
        throw new Error(
          "The bank engine did not return the accepted document.",
        );
      }
      const next = await projectDocument(
        opening.name,
        reply,
        opening.breadcrumbs,
      );
      if (!stillOpening()) throw new Error("The selected bank changed.");
      if (currentDraft.current) {
        throw new Error("Discard the previous bank draft before finishing.");
      }
      acceptedOpen.current = null;
      setOpenRecovery(undefined);
      setProfile(undefined);
      setOverwriteAvailable(opening.canOverwrite);
      setSelectedBankId(opening.bankId);
      publishDocument(next);
      return { ok: true, message: `Opened ${opening.name}.` };
    } catch (error) {
      if (!stillOpening()) throw error;
      const target =
        error instanceof SnapshotRefreshRequired &&
        error.target.document_id === opening.target.document_id &&
        revisionAtLeast(
          error.target.expected_revision,
          opening.target.expected_revision,
        )
          ? error.target
          : opening.target;
      const updated = { ...opening, target };
      acceptedOpen.current = updated;
      setOpenRecovery(updated);
      return {
        ok: false,
        message: currentDraft.current
          ? "Discard the previous bank draft before finishing."
          : "The bank was accepted, but its XML could not be displayed.",
      };
    }
  }

  const inventory = useInventory({
    backupGeneration,
    selectedBankId,
    disabled:
      busy ||
      hasDraft ||
      openRecovery !== undefined ||
      refreshTarget !== undefined ||
      closeGuard.pending,
    onOpenBank: (id) => {
      void openFrom((control) => openInventoryBank(id, control), id).then(
        (result) => setNativeNotice(result.message),
      );
    },
  });
  const selectedMap =
    inventory.status === "ready"
      ? inventory.maps.find(({ id }) => id === inventory.selectedMapId)
      : undefined;
  const inventoryWithBackupStatus =
    inventory.status === "ready" && inventory.bankBrowser
      ? {
          ...inventory,
          bankBrowser: {
            ...inventory.bankBrowser,
            activeBackupStatus: backupStatus,
          },
        }
      : inventory;

  function save(target: EditorTarget, mode: "overwrite" | "backup" | "new") {
    return action(() => {
      activeDocument(target);
      return saveCurrent(target, mode);
    });
  }

  async function saveCurrent(
    target: EditorTarget,
    mode: "overwrite" | "backup" | "new",
  ): Promise<EditorActionResult> {
    try {
      return await saveRevision(target, mode);
    } catch (error) {
      if (error !== "external_bank_changed" || mode === "new") throw error;
      return resolveConflict(target);
    }
  }

  async function saveRevision(
    target: EditorTarget,
    mode: "overwrite" | "backup" | "new",
  ): Promise<EditorActionResult> {
    const nativeTarget = activeTarget(target);
    const saved =
      mode === "new"
        ? await saveBankAs(nativeTarget)
        : await saveActiveBank(nativeTarget, mode === "backup");
    if (!saved) return { ok: true };
    await adopt(
      saved.name,
      decodeEngineReply(saved.control, nativeTarget.requestId),
      saved.breadcrumbs,
    );
    setOverwriteAvailable(saved.can_overwrite === true);
    if (mode === "new") setSelectedBankId(undefined);
    else setBackupGeneration((value) => value + 1);
    return {
      ok: true,
      message:
        mode === "backup"
          ? `Saved ${saved.name} with a backup.`
          : `Saved ${saved.name}.`,
    };
  }

  function resolveConflict(target: EditorTarget) {
    return new Promise<EditorActionResult>((resolve) => {
      function finish(result: EditorActionResult) {
        setConflictPrompt(undefined);
        resolve(result);
      }
      setConflictPrompt({
        compare: () => compareBankConflict(activeTarget(target)),
        reopen: async (reviewId) => {
          const nativeTarget = activeTarget(target);
          const opened = await reopenBankConflict(nativeTarget, reviewId);
          if (!opened) return;
          await adopt(
            opened.name,
            decodeEngineReply(opened.control, nativeTarget.requestId),
            opened.breadcrumbs,
          );
          setOverwriteAvailable(opened.can_overwrite === true);
          setBackupGeneration((value) => value + 1);
          finish({ ok: false, message: `Reopened ${opened.name} from disk.` });
        },
        saveAs: exportAvailable
          ? async () => {
              const saved = await saveRevision(target, "new");
              if (saved.ok && saved.message !== undefined) {
                finish({ ok: false, message: saved.message });
              }
            }
          : undefined,
        cancel: () =>
          finish({
            ok: false,
            message:
              "The bank changed on disk. The current editor remained open.",
          }),
        messageFor,
      });
    });
  }

  function browseBackups(target: EditorTarget) {
    return action(async () => {
      const nativeTarget = activeTarget(target);
      const choices = await listActiveBackups(nativeTarget);
      const selection = await new Promise<BackupAction | undefined>(
        (resolve) => {
          setBackupPrompt({
            choices,
            reviewRestore: overwriteAvailable
              ? (id) => reviewActiveBackup(nativeTarget, id)
              : undefined,
            select(selection) {
              setBackupPrompt(undefined);
              resolve(selection);
            },
          });
        },
      );
      if (!selection) return { ok: true };
      if (selection.kind === "export") {
        const name = await exportActiveBackup(nativeTarget, selection.id);
        return {
          ok: true,
          message: name ? `Saved backup copy ${name}.` : undefined,
        };
      }
      const restored = await restoreActiveBank(nativeTarget, selection.id);
      if (!restored) return { ok: true };
      await adopt(
        restored.name,
        decodeEngineReply(restored.control, nativeTarget.requestId),
        restored.breadcrumbs,
      );
      setOverwriteAvailable(restored.can_overwrite === true);
      setBackupGeneration((value) => value + 1);
      return {
        ok: true,
        message: `Restored ${restored.name} and saved a reversal backup.`,
      };
    });
  }

  const controller: BankEditorController = {
    document: document?.projection,
    busy: busy || closeGuard.pending,
    status: nativeNotice,
    backupStatus,
    onOpen: () => openFrom(pickBank),
    onDraftChange: closeGuard.reportDraft,
    onRefreshSnapshot:
      !openRecovery && refreshTarget
        ? (target) =>
            updateDocument(
              target,
              async () =>
                (
                  await dispatch(
                    {
                      id: refreshTarget.document_id,
                      revision: refreshTarget.expected_revision,
                    },
                    { kind: "snapshot" },
                  )
                ).reply,
              refreshTarget.expected_revision,
            )
        : undefined,
    profile: {
      current: profile ?? { status: "unloaded" },
      requireAuthor: profile?.status === "missing_context",
      onLoad: (target, file, author) =>
        action(async () => {
          if (!document || document.projection.id !== target.id) {
            throw new Error("Open a bank before loading a profile.");
          }
          const payload = profileLoadForFile(
            file,
            document.projection.name,
            author,
          );
          const bytes = new Uint8Array(await file.arrayBuffer());
          await updateProfile(target, { kind: "profile_load", payload }, bytes);
          return { ok: true };
        }),
      onClear: (target) =>
        action(async () => {
          await updateProfile(target, { kind: "profile_clear" });
          return { ok: true };
        }),
      onPreview: async (target, payload) => {
        const { reply } = await dispatch(target, {
          kind: "profile_preview",
          payload,
        });
        if (
          reply.result.kind !== "profile_preview" ||
          reply.document_id !== target.id ||
          reply.revision !== target.revision
        ) {
          throw new Error(
            "The bank changed before the profile preview could be read.",
          );
        }
        return reply.result.payload;
      },
      onApply: (target, preview_id) =>
        update(target, { kind: "profile_apply", payload: { preview_id } }),
    },
    onStructure: (target, change) =>
      update(target, { kind: "structure", payload: change }),
    onRevert: (target, selection) =>
      update(target, { kind: "revert", payload: selection }),
    onDiff: async (target) => {
      let diff: EngineDiff | undefined;
      const outcome = await action(async () => {
        const { reply } = await dispatch(target, { kind: "diff" });
        if (
          reply.result.kind !== "diff" ||
          reply.document_id !== target.id ||
          reply.revision !== target.revision
        ) {
          return {
            ok: false,
            message: "The bank changed before its diff could be read.",
          };
        }
        diff = reply.result.payload;
        return { ok: true };
      });
      if (diff) return { ok: true, diff };
      return {
        ok: false,
        message: outcome.message ?? "The bank diff could not be read.",
      };
    },
    onValidateValues: async (target, edits) => {
      const { reply, bytes } = await dispatch(target, {
        kind: "validate_values",
        payload: { edits },
      });
      if (
        reply.result.kind !== "value_validation" ||
        reply.document_id !== target.id ||
        reply.revision !== target.revision ||
        bytes !== undefined
      ) {
        throw new Error(
          "The bank engine did not check edits for this revision.",
        );
      }
      return reply.result.payload.issues;
    },
    onEditValues: (target, edits) =>
      update(target, { kind: "set_values", payload: { edits } }),
    onReplaceXml: (target, xml) =>
      update(target, { kind: "replace_raw_text" }, encodeBankXml(xml)),
    onReset: (target) => update(target, { kind: "reset" }),
    onRevertToSaved: (target) => update(target, { kind: "revert_to_saved" }),
    onUndo: document?.canUndo
      ? (target) => update(target, { kind: "undo" })
      : undefined,
    onRedo: document?.canRedo
      ? (target) => update(target, { kind: "redo" })
      : undefined,
    onSave: overwriteAvailable
      ? (target) => save(target, "overwrite")
      : undefined,
    onSaveAs: exportAvailable ? (target) => save(target, "new") : undefined,
    onSaveWithBackup: overwriteAvailable
      ? (target) => save(target, "backup")
      : undefined,
    onBackupHistory: exportAvailable ? browseBackups : undefined,
    canRestoreBackup: overwriteAvailable,
    onReveal: revealAvailable
      ? (target) =>
          action(async () => {
            await revealActiveBank(activeTarget(target));
            return { ok: true };
          })
      : undefined,
    onResign: (target) =>
      action(() =>
        resignOpenBank({
          document: document?.projection,
          target,
          activeTarget,
          adopt: async (name, reply) => {
            await adopt(name, reply);
          },
          setPrompt: setSigningPrompt,
          errorMessage: messageFor,
        }),
      ),
    onExport: exportAvailable
      ? (target) =>
          action(async () => {
            const { projection } = activeDocument(target);
            const id = requestId();
            const control = engineRequest(
              id,
              { kind: "prepare" },
              {
                document_id: target.id,
                expected_revision: target.revision,
              },
            );
            const exported = await exportBankCopy(
              JSON.stringify(control),
              projection.name,
            );
            if (!exported) return { ok: true };
            const { result } = decodeEngineReply(exported.control, id);
            if (result.kind === "error")
              throw new Error(result.payload.message);
            if (result.kind !== "prepared") {
              throw new Error("The bank engine did not confirm the export.");
            }
            return { ok: true, message: `Exported ${exported.name}.` };
          })
      : undefined,
    onCopy: navigator.clipboard
      ? (target) =>
          action(async () => {
            activeDocument(target);
            const { reply, bytes } = await dispatch(target, {
              kind: "prepare",
            });
            if (
              reply.result.kind !== "prepared" ||
              reply.document_id !== target.id ||
              reply.revision !== target.revision ||
              bytes === undefined
            ) {
              throw new Error("The bank engine did not prepare XML.");
            }
            const status = reply.result.payload.signature_status;
            if (
              status !== "valid" &&
              status !== "unsigned" &&
              !window.confirm(
                "Copy XML with an unverified signature? StarCraft II may refuse it.",
              )
            ) {
              return { ok: true };
            }
            await navigator.clipboard.writeText(decodeBankFileXml(bytes));
            return { ok: true, message: "Copied bank XML." };
          })
      : undefined,
  };

  function updateBackupSettings(
    command: Parameters<typeof backupPreferences.update>[0],
    args?: { keepNewest: number | null },
  ) {
    void action(async () => {
      const changed = await backupPreferences.update(command, args);
      if (changed && command !== "set_backup_retention")
        setBackupGeneration((value) => value + 1);
      return { ok: true };
    });
  }

  return (
    <>
      <AppShell
        platformInfo={__STARBANK_PLATFORM_INFO__}
        name={APP_NAME}
        editor={controller}
        openRecovery={
          openRecovery && {
            name: openRecovery.name,
            finish: () =>
              action(() =>
                displayOpening(
                  openRecovery,
                  async () =>
                    (
                      await dispatch(
                        {
                          id: openRecovery.target.document_id,
                          revision: openRecovery.target.expected_revision,
                        },
                        { kind: "snapshot" },
                      )
                    ).reply,
                ),
              ),
          }
        }
        inventory={inventoryWithBackupStatus}
        preferences={preferences}
        backupSettings={
          exportAvailable && (
            <>
              <BackupPreferencesFields
                preferences={backupPreferences.preferences}
                issue={backupPreferences.issue}
                disabled={busy || closeGuard.pending}
                choose={() => updateBackupSettings("choose_backup_location")}
                useDefault={() =>
                  updateBackupSettings("use_default_backup_location")
                }
                setKeepNewest={(keepNewest) =>
                  updateBackupSettings("set_backup_retention", { keepNewest })
                }
              />
              {backupPreferences.preferences && (
                <BackupRetentionReview
                  key={backupGeneration}
                  target={
                    document && {
                      documentId: document.projection.id,
                      revision: document.projection.revision,
                      requestId: `retention-${document.projection.revision}`,
                    }
                  }
                  keepNewest={backupPreferences.preferences.keepNewest}
                  disabled={
                    busy ||
                    openRecovery !== undefined ||
                    closeGuard.pending ||
                    backupPreferences.preferences.location.mode ===
                      "needs_authorization"
                  }
                />
              )}
            </>
          )
        }
        breadcrumbAvailable
        tools={
          <BatchBackupTools
            onResignExternal={
              exportAvailable && !busy && !closeGuard.pending
                ? () => {
                    void action(() =>
                      resignExternalBank({
                        setPrompt: setSigningPrompt,
                        errorMessage: messageFor,
                      }),
                    ).then((result) => setNativeNotice(result.message));
                  }
                : undefined
            }
            externalSigningUnavailable={
              exportAvailable
                ? undefined
                : "Signed copy export is unavailable on this platform."
            }
            availability={
              !exportAvailable
                ? {
                    kind: "unavailable",
                    reason: "Batch backup is unavailable on this platform.",
                  }
                : inventory.status === "ready" &&
                    inventory.bankBrowser?.banks.length
                  ? { kind: "ready", selectedMap }
                  : {
                      kind: "unavailable",
                      reason: "Load a bank inventory to use batch backup.",
                    }
            }
            onStart={(request) =>
              exclusive(async () => {
                const report = await createBatchBackup(request);
                if (report?.created) setBackupGeneration((value) => value + 1);
                return report;
              })
            }
            onCancel={() => invoke<void>("cancel_batch_backup")}
          />
        }
        workspaceAccessory={
          <OpenRequests
            busy={busy || openRecovery !== undefined}
            hasDraft={hasDraft}
            onOpen={(id) =>
              openFrom((control) => openRequestedBank(id, control))
            }
          />
        }
      />
      {signingPrompt ? <ManualSigningSheet {...signingPrompt} /> : null}
      {conflictPrompt ? <BankConflictDialog {...conflictPrompt} /> : null}
      {backupPrompt ? (
        <BackupPicker {...backupPrompt} messageFor={messageFor} />
      ) : null}
    </>
  );
}

function requireSnapshot(reply: EngineReply): {
  target: EditorTarget;
  snapshot: EngineSnapshot;
} {
  if (reply.result.kind === "error") {
    throw new Error(reply.result.payload.message);
  }
  if (
    reply.result.kind !== "snapshot" ||
    reply.document_id === null ||
    reply.revision === null
  ) {
    throw new Error("The bank engine did not return a document.");
  }
  return {
    target: { id: reply.document_id, revision: reply.revision },
    snapshot: reply.result.payload,
  };
}

function displaySections(snapshot: EngineSnapshot) {
  return snapshot.sections.map((section) => ({
    id: section.id,
    name: section.name ?? "Unnamed section",
    dirty: section.dirty,
    keys: section.keys.map((key) => ({
      id: key.id,
      name: key.name ?? "Unnamed key",
    })),
    rows: section.keys.flatMap((key): EditorRow[] => {
      const name = key.name ?? "Unnamed key";
      if (key.values.length === 0) {
        return [
          {
            id: key.id,
            keyId: key.id,
            name,
            type: "",
            value: "",
            editable: false,
          },
        ];
      }
      return key.values.map((value) => ({
        id: value.id,
        keyId: key.id,
        name,
        childName: value.name === "Value" ? undefined : value.name,
        type: value.kind ?? "",
        value: value.value ?? "",
        editable: value.editable,
      }));
    }),
  }));
}

function messageFor(error: unknown): string {
  switch (error) {
    case "external_bank_changed":
      return "The bank changed on disk.";
    case "conflict_review_changed":
      return "The bank changed during this review. Compare again before reopening.";
    case "reveal_unavailable":
      return "Open in Folder is unavailable on this platform.";
    case "reveal_invalid_target":
    case "reveal_target_changed":
      return "The selected bank changed. Try Open in Folder again.";
    case "reveal_file_changed":
      return "The bank file changed on disk. Reopen it to use Open in Folder.";
    case "reveal_failed":
      return "The file manager could not open the bank folder.";
  }
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error === "dialog_busy") {
    return "A file dialog is already open.";
  }
  if (error === "stale_bank" || error === "unknown_bank") {
    return "The selected bank changed. Refresh the inventory and select it again.";
  }
  if (error === "active_write_uncertain") {
    return "The bank write could not be verified. Inspect the file and recovery copies before reopening it.";
  }
  if (error === "new_file_uncertain") {
    return "The new file could not be verified. Inspect the destination and its recovery copies.";
  }
  if (error === "saved_needs_reopen") {
    return "The write was verified on disk. Reopen the bank to refresh the editor.";
  }
  if (
    typeof error === "object" &&
    error !== null &&
    "active_write_rejected" in error &&
    typeof error.active_write_rejected === "string"
  ) {
    return `The bank operation was refused. ${error.active_write_rejected}`;
  }
  if (
    typeof error === "object" &&
    error !== null &&
    "cannot_compare_bank" in error &&
    typeof error.cannot_compare_bank === "string"
  ) {
    return `The bank on disk could not be reviewed. ${error.cannot_compare_bank}`;
  }
  if (error === "write_uncertain") {
    return "The export outcome is uncertain. Inspect the destination before retrying.";
  }
  if (
    typeof error === "object" &&
    error !== null &&
    "write_rejected" in error &&
    typeof error.write_rejected === "string"
  ) {
    return `The export was refused. ${error.write_rejected}`;
  }
  if (typeof error === "object" && error !== null && "file" in error) {
    if (error.file === "too_large") return "The selected bank is too large.";
    return "Select a readable StarCraft II bank file.";
  }
  return "The native bank operation failed.";
}

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing StarBank application root");
}
createRoot(root).render(<DesktopApp />);
