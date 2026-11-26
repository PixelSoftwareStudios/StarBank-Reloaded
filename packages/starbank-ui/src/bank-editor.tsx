import { Tabs } from "@heroui/react/tabs";
import {
  lazy,
  type ReactNode,
  Suspense,
  useLayoutEffect,
  useState,
} from "react";
import {
  type BankEditorController,
  type BankEditorDocument,
  type EditorProfileField,
  type EditorProfileInput,
  type EditorRow,
  prepareTableDraft,
  profileStatus,
  useEditorState,
} from "starbank-editor";
import type { EngineDiff } from "starbank-engine-client/protocol";
import type { AppSettings } from "starbank-platform/settings";
import { BankBreadcrumbs } from "./bank-breadcrumbs";
import { BankRevealButton } from "./bank-reveal-button";
import {
  EditorActions,
  fileDropHandlers,
  OpenBankButton,
  type RunEditorAction,
  SettingsButton,
} from "./editor-actions";
import { ProfilePicker, SimpleEditor } from "./simple-editor";
import { TableEditor } from "./table-editor";

export type EditorSettings = Pick<
  AppSettings,
  "sectionsLayout" | "wrapRaw" | "groupNumbers" | "showBreadcrumbs"
>;

let rawEditorModule: Promise<typeof import("./raw-editor")> | undefined;
let structureEditorModule:
  | Promise<typeof import("./structure-editor")>
  | undefined;
let editorDiffModule: Promise<typeof import("./editor-diff")> | undefined;

export function preloadRawEditor() {
  rawEditorModule ??= import("./raw-editor");
  return rawEditorModule;
}

function preloadStructureEditor() {
  structureEditorModule ??= import("./structure-editor");
  return structureEditorModule;
}

function preloadEditorDiff() {
  editorDiffModule ??= import("./editor-diff");
  return editorDiffModule;
}

export function preloadEditorTools() {
  return Promise.all([
    preloadRawEditor(),
    preloadStructureEditor(),
    preloadEditorDiff(),
  ]);
}

const RawEditor = lazy(preloadRawEditor);
const StructureEditor = lazy(preloadStructureEditor);
const EditorDiff = lazy(preloadEditorDiff);

export function BankEditor({
  controller,
  document,
  busy,
  notice,
  clearNotice,
  run,
  settings,
  onOpenSettings,
  tools,
}: {
  controller: BankEditorController;
  document: BankEditorDocument;
  busy: boolean;
  notice?: string;
  clearNotice: () => void;
  run: RunEditorAction;
  settings?: EditorSettings;
  onOpenSettings?: () => void;
  tools?: ReactNode;
}) {
  const { draft, profileDraft, view, setDraft, setProfileDraft, setView } =
    useEditorState();
  const profileController = controller.profile;
  const profile =
    profileController?.current.status === "matched"
      ? profileController.current
      : undefined;
  const selectedView =
    view === "simple" && !profile
      ? "table"
      : (view ?? (profile ? "simple" : "table"));
  const [structure, setStructure] = useState<{
    revision: string;
    sectionId?: string;
  }>();
  const [diff, setDiff] = useState<{ revision: string; value: EngineDiff }>();
  const target = { id: document.id, revision: document.revision };
  const tableDraft = draft?.kind === "table" ? draft : undefined;
  const rawDraft = draft?.kind === "raw" ? draft : undefined;
  const prepared = tableDraft
    ? prepareTableDraft(document.sections, tableDraft.values)
    : undefined;
  const stale = draft !== undefined && draft.revision !== document.revision;
  const {
    onEditValues,
    onReplaceXml,
    onDraftChange,
    onStructure,
    onDiff,
    onRevert,
  } = controller;
  const hasDraft = draft !== undefined || profileDraft !== undefined;
  const pending = busy || hasDraft;
  useLayoutEffect(() => {
    onDraftChange?.(draft !== undefined || profileDraft !== undefined);
  }, [draft, profileDraft, onDraftChange]);

  const staleProfile =
    profileDraft !== undefined &&
    (profileDraft.revision !== document.revision ||
      profileDraft.profile_id !== profile?.profile_id);

  function stageProfile(field: EditorProfileField, input: EditorProfileInput) {
    if (!profile || busy || draft || staleProfile || field.read_only) return;
    const original = input.kind === "checked" ? field.checked : field.value;
    setProfileDraft(
      input.value === original
        ? undefined
        : {
            profile_id: profile.profile_id,
            revision: document.revision,
            field_id: field.id,
            input,
          },
    );
  }

  function previewProfile() {
    if (!profileController || !profileDraft || staleProfile || busy) return;
    const proposed = profileDraft;
    run(async () => {
      const preview = await profileController.onPreview(target, {
        profile_id: proposed.profile_id,
        field_id: proposed.field_id,
        input: proposed.input,
      });
      setProfileDraft((current) =>
        current === proposed
          ? preview.changes.length > 0
            ? { ...current, preview }
            : undefined
          : current,
      );
      return preview.changes.length > 0
        ? { ok: true }
        : { ok: true, message: "The profile values are already current." };
    });
  }

  function applyProfile() {
    const preview = profileDraft?.preview;
    if (!preview || !profileController || staleProfile || busy) return;
    run(
      () => profileController.onApply(target, preview.preview_id),
      () => setProfileDraft(undefined),
    );
  }

  function stageValues(rows: readonly EditorRow[], value: string) {
    setDraft((current) => {
      if (
        profileDraft ||
        current?.kind === "raw" ||
        (current && current.revision !== document.revision)
      )
        return current;
      const values = new Map(current?.values);
      for (const row of rows) {
        if (value === row.value) values.delete(row.id);
        else values.set(row.id, value);
      }
      return values.size > 0
        ? { kind: "table", revision: document.revision, values }
        : undefined;
    });
  }

  function apply() {
    const clearDraft = () => setDraft(undefined);
    if (busy || stale) return;
    if (rawDraft && onReplaceXml) {
      run(() => onReplaceXml(target, rawDraft.text), clearDraft);
    } else if (prepared && prepared.invalid.size === 0 && onEditValues) {
      if (prepared.edits.length > 0)
        run(() => onEditValues(target, prepared.edits), clearDraft);
      else clearDraft();
    }
  }

  const status =
    notice ||
    controller.status ||
    profileStatus(profileController?.current) ||
    (document.dirty ? "Unsaved changes" : "");

  return (
    <div
      className="document-editor"
      {...fileDropHandlers(controller.onOpenFile, pending, run)}
    >
      <header className="editor-header">
        <div className="editor-header-bar">
          {document.breadcrumbs?.length &&
          settings?.showBreadcrumbs !== false ? (
            <BankBreadcrumbs segments={document.breadcrumbs} />
          ) : null}
          <div className="editor-open-actions">
            <OpenBankButton
              controller={controller}
              disabled={pending}
              run={run}
            />
            <BankRevealButton
              reveal={controller.onReveal}
              target={target}
              busy={pending}
              run={run}
            />
            {tools}
            <SettingsButton open={onOpenSettings} />
          </div>
        </div>
        <h1 title={document.name}>{document.name}</h1>
        {profileController && (
          <ProfilePicker
            controller={profileController}
            target={target}
            disabled={pending}
            run={run}
          />
        )}
      </header>

      <Tabs
        className="editor-views"
        selectedKey={selectedView}
        onSelectionChange={(key) => {
          if (key === "simple" || key === "table" || key === "raw")
            setView(key);
        }}
        variant="secondary"
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label="Bank views">
            {profile && (
              <Tabs.Tab id="simple">
                Simple View
                <Tabs.Indicator />
              </Tabs.Tab>
            )}
            <Tabs.Tab id="table">
              Table View
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="raw">
              Raw View
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
        {profile && (
          <Tabs.Panel id="simple" className="editor-view">
            <SimpleEditor
              profile={profile}
              draft={profileDraft}
              disabled={busy || Boolean(draft) || staleProfile}
              onChange={stageProfile}
            />
          </Tabs.Panel>
        )}
        <Tabs.Panel id="table" className="editor-view">
          <TableEditor
            sections={document.sections}
            values={tableDraft?.values}
            invalid={prepared?.invalid}
            canEdit={
              Boolean(onEditValues) &&
              !busy &&
              !stale &&
              !rawDraft &&
              !profileDraft
            }
            onChange={stageValues}
            layout={settings?.sectionsLayout}
            groupNumbers={settings?.groupNumbers}
            actionsDisabled={pending}
            onOpenStructure={
              onStructure &&
              ((sectionId) => {
                clearNotice();
                setStructure({ revision: document.revision, sectionId });
              })
            }
            onOpenDiff={
              onDiff &&
              (() => {
                run(async () => {
                  const result = await onDiff(target);
                  if (result.ok)
                    setDiff({ revision: target.revision, value: result.diff });
                  return result;
                });
              })
            }
          />
        </Tabs.Panel>
        <Tabs.Panel id="raw" className="editor-view">
          <Suspense fallback={<p role="status">Loading XML editor…</p>}>
            <RawEditor
              key={document.revision}
              value={rawDraft?.text ?? document.xml}
              wrap={settings?.wrapRaw}
              readOnly={
                !onReplaceXml ||
                busy ||
                stale ||
                tableDraft !== undefined ||
                profileDraft !== undefined
              }
              onChange={(text) => {
                setDraft(
                  text === document.xml
                    ? undefined
                    : { kind: "raw", revision: document.revision, text },
                );
              }}
            />
          </Suspense>
        </Tabs.Panel>
      </Tabs>

      <Suspense fallback={null}>
        {onStructure && structure?.revision === document.revision && (
          <StructureEditor
            sections={document.sections}
            initialSection={structure.sectionId}
            busy={busy}
            notice={notice}
            onApply={(change) =>
              run(
                () => onStructure(target, change),
                () => setStructure(undefined),
              )
            }
            onDismiss={() => setStructure(undefined)}
          />
        )}
        {diff?.revision === document.revision && (
          <EditorDiff
            diff={diff.value}
            busy={busy}
            notice={notice}
            onRevert={
              onRevert &&
              ((selection) =>
                run(
                  () => onRevert(target, selection),
                  () => setDiff(undefined),
                ))
            }
            onDismiss={() => setDiff(undefined)}
          />
        )}
      </Suspense>

      {profileDraft && (
        <div className="editor-draft-actions">
          <p role="status">
            {staleProfile
              ? "The bank or profile changed. Discard this draft to continue."
              : "Unapplied Simple View changes"}
          </p>
          <button
            className="editor-button"
            type="button"
            disabled={busy}
            onClick={() => {
              setProfileDraft(undefined);
              clearNotice();
            }}
          >
            Discard draft
          </button>
          <button
            className="editor-button editor-primary"
            type="button"
            disabled={busy || staleProfile}
            onClick={profileDraft.preview ? applyProfile : previewProfile}
          >
            {profileDraft.preview ? "Apply preview" : "Preview edits"}
          </button>
        </div>
      )}

      {draft && (
        <div className="editor-draft-actions">
          <p role="status">
            {stale
              ? "The bank changed while you were editing. Discard this draft to continue."
              : draft.kind === "raw"
                ? "Unapplied Raw View changes"
                : "Unapplied Table View changes"}
          </p>
          <button
            className="editor-button"
            type="button"
            disabled={busy}
            onClick={() => {
              setDraft(undefined);
              clearNotice();
            }}
          >
            Discard draft
          </button>
          <button
            className="editor-button editor-primary"
            type="button"
            disabled={busy || stale || Boolean(prepared?.invalid.size)}
            onClick={apply}
          >
            {draft.kind === "raw" ? "Apply XML" : "Apply edits"}
          </button>
        </div>
      )}

      <footer className="editor-footer">
        <div className="editor-status" role="status">
          {busy ? "Working…" : status}
          {document.signatureStatus && <span>{document.signatureStatus}</span>}
        </div>
        <EditorActions
          controller={controller}
          target={target}
          disabled={pending}
          run={run}
        />
      </footer>
    </div>
  );
}
