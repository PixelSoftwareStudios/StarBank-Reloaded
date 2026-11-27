import * as Tabs from "@radix-ui/react-tabs";
import {
  lazy,
  type ReactNode,
  Suspense,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  type BankEditorController,
  type BankEditorDocument,
  type EditorActionResult,
  type EditorDraft,
  type EditorProfileField,
  type EditorProfileInput,
  type EditorRow,
  prepareTableDraft,
  profileStatus,
  useEditorState,
} from "starbank-editor";
import type { EngineDiff } from "starbank-engine-client/protocol";
import type { AppSettings } from "starbank-platform/settings";
import { type MapContext, MapContextDetails } from "../inventory/map-context";
import { BankBreadcrumbs } from "./bank-breadcrumbs";
import { BankRevealButton } from "./bank-reveal-button";
import { DraftSwitchDialog, type DraftSwitchKind } from "./draft-switch-dialog";
import {
  EditorActions,
  fileDropHandlers,
  OpenBankButton,
  type RunEditorAction,
  SettingsButton,
} from "./editor-actions";
import type { RawEditorHandle } from "./raw-editor";
import { ProfilePicker, SimpleEditor } from "./simple-editor";
import { TableEditor } from "./table-editor";

type EditorView = NonNullable<ReturnType<typeof useEditorState>["view"]>;

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
  mapContext,
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
  mapContext?: MapContext;
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
  const [draftSwitch, setDraftSwitch] = useState<{
    kind: DraftSwitchKind;
    nextView: EditorView;
    message: string;
  }>();
  const simplePanel = useRef<HTMLDivElement>(null);
  const tablePanel = useRef<HTMLDivElement>(null);
  const [retainTable, setRetainTable] = useState(false);
  const rawEditor = useRef<RawEditorHandle>(null);
  const [retainedRawRevision, setRetainedRawRevision] = useState<string>();
  const [rawRestart, setRawRestart] = useState(false);
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

  const simpleVisible = selectedView === "simple";
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
    if (
      !simpleVisible ||
      !profileController ||
      !profileDraft ||
      staleProfile ||
      busy
    )
      return;
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
    if (
      !simpleVisible ||
      !preview ||
      !profileController ||
      staleProfile ||
      busy
    )
      return;
    run(
      () => profileController.onApply(target, preview.preview_id),
      () => setProfileDraft(undefined),
    );
  }

  function discardProfileDraft() {
    setProfileDraft(undefined);
    clearNotice();
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

  function showView(nextView: EditorView) {
    if (selectedView === "table") setRetainTable(true);
    if (selectedView === "raw" && !rawDraft)
      setRetainedRawRevision(document.revision);
    setView(nextView);
  }

  function finishDraft(nextView?: EditorView) {
    setDraft(undefined);
    if (nextView) showView(nextView);
  }

  function submitDraft(
    kind: EditorDraft["kind"],
    action: () => Promise<EditorActionResult>,
    nextView?: EditorView,
  ) {
    run(
      async () => {
        let result: EditorActionResult;
        try {
          result = await action();
        } catch (error) {
          result = {
            ok: false,
            message:
              error instanceof Error
                ? error.message
                : `The ${kind === "raw" ? "Raw" : "Table"} draft could not be applied.`,
          };
        }
        if (!result.ok && nextView)
          setDraftSwitch({ kind, nextView, message: result.message });
        return result;
      },
      () => finishDraft(nextView),
    );
  }

  function applyTable(nextView?: EditorView) {
    if (!prepared || prepared.invalid.size > 0 || !onEditValues) return;
    if (prepared.edits.length > 0)
      submitDraft(
        "table",
        () => onEditValues(target, prepared.edits),
        nextView,
      );
    else finishDraft(nextView);
  }

  function apply() {
    if (busy || stale) return;
    if (rawDraft && onReplaceXml)
      submitDraft("raw", () => onReplaceXml(target, rawDraft.text));
    else if (tableDraft) applyTable();
  }

  function discardDraft() {
    setDraft(undefined);
    clearNotice();
    if (rawDraft) {
      setRetainedRawRevision(undefined);
      setRawRestart((previous) => !previous);
    }
  }

  function selectView(nextView: EditorView) {
    if (nextView === selectedView || (hasDraft && busy)) return;
    const staleMessage = "The bank changed while you were editing this draft.";
    if (simpleVisible && profileDraft) {
      setDraftSwitch({
        kind: "simple",
        nextView,
        message: staleProfile
          ? "The bank or profile changed. Discard this draft to continue."
          : profileDraft.preview
            ? "Apply the reviewed edits in Simple View before switching."
            : "Preview the edits in Simple View before applying them.",
      });
    } else if (selectedView === "raw" && rawDraft) {
      setRetainedRawRevision(undefined);
      if (stale || !onReplaceXml) {
        setDraftSwitch({
          kind: "raw",
          nextView,
          message: stale
            ? staleMessage
            : "The Raw draft cannot currently be applied.",
        });
      } else {
        submitDraft("raw", () => onReplaceXml(target, rawDraft.text), nextView);
      }
    } else if (selectedView === "table" && prepared) {
      const message = stale
        ? staleMessage
        : !onEditValues
          ? "The Table draft cannot currently be applied."
          : prepared.invalid.size > 0
            ? "Check the marked values. Use commas only between three-digit groups."
            : undefined;
      if (message) setDraftSwitch({ kind: "table", nextView, message });
      else applyTable(nextView);
    } else {
      showView(nextView);
    }
  }

  const tableVisible = selectedView === "table";
  const rawVisible = selectedView === "raw";
  const keepRawMounted =
    rawVisible || retainedRawRevision === document.revision;
  const status = busy
    ? "Working…"
    : notice || controller.status || profileStatus(profileController?.current);
  const dirtyCount = document.dirtySectionCount ?? 0;
  const dirtyStatus = document.dirty
    ? dirtyCount > 0
      ? `Unsaved changes in ${dirtyCount} ${dirtyCount === 1 ? "section" : "sections"}`
      : "Unsaved changes"
    : undefined;
  const showBreadcrumbs =
    Boolean(document.breadcrumbs?.length) &&
    settings?.showBreadcrumbs !== false;
  const title = mapContext?.map.name ?? document.name;

  return (
    <div
      className="document-editor"
      {...fileDropHandlers(controller.onOpenFile, pending, run)}
    >
      <header className="editor-header">
        <div className="editor-header-bar">
          {showBreadcrumbs && document.breadcrumbs && (
            <BankBreadcrumbs segments={document.breadcrumbs} />
          )}
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
        <h1 title={title}>{title}</h1>
        {mapContext && (
          <MapContextDetails
            context={mapContext}
            disabled={pending}
            showBankName={!showBreadcrumbs}
          />
        )}
        {profileController && (
          <ProfilePicker
            controller={profileController}
            target={target}
            disabled={pending}
            run={run}
          />
        )}
      </header>

      <Tabs.Root
        className="editor-views"
        value={selectedView}
        activationMode={hasDraft ? "manual" : "automatic"}
        onValueChange={(key) => {
          if (key === "simple" || key === "table" || key === "raw")
            selectView(key);
        }}
      >
        <Tabs.List
          aria-label="Bank views"
          className="editor-tab-list"
          onKeyDown={(event) => {
            // Radix activates the tab from keydown. Its native button click can
            // otherwise activate the newly focused dialog button.
            if (hasDraft && (event.key === "Enter" || event.key === " "))
              event.preventDefault();
          }}
        >
          {profile && (
            <Tabs.Trigger className="editor-tab" value="simple">
              Simple View
            </Tabs.Trigger>
          )}
          <Tabs.Trigger className="editor-tab" value="table">
            Table View
          </Tabs.Trigger>
          <Tabs.Trigger className="editor-tab" value="raw">
            Raw View
          </Tabs.Trigger>
        </Tabs.List>
        {profile && (
          <Tabs.Content
            value="simple"
            className="editor-view"
            ref={simplePanel}
          >
            <SimpleEditor
              profile={profile}
              draft={profileDraft}
              disabled={busy || Boolean(draft) || staleProfile}
              onChange={stageProfile}
            />
          </Tabs.Content>
        )}
        <Tabs.Content
          value="table"
          className="editor-view"
          forceMount={tableVisible || retainTable ? true : undefined}
          hidden={!tableVisible}
          ref={tablePanel}
        >
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
        </Tabs.Content>
        <Tabs.Content
          value="raw"
          className="editor-view"
          forceMount={keepRawMounted ? true : undefined}
          hidden={!rawVisible}
        >
          <Suspense fallback={<p role="status">Loading XML editor…</p>}>
            <RawEditor
              key={`${document.revision}:${rawRestart}`}
              ref={rawEditor}
              value={rawDraft?.text ?? document.xml}
              wrap={settings?.wrapRaw}
              readOnly={
                !rawVisible ||
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
        </Tabs.Content>
      </Tabs.Root>

      {draftSwitch && (
        <DraftSwitchDialog
          kind={draftSwitch.kind}
          message={draftSwitch.message}
          repairable={
            draftSwitch.kind === "simple"
              ? !staleProfile
              : !stale &&
                Boolean(
                  draftSwitch.kind === "raw" ? onReplaceXml : onEditValues,
                )
          }
          onDecision={(decision) => {
            if (decision === "discard") {
              if (draftSwitch.kind === "simple") discardProfileDraft();
              else discardDraft();
              showView(draftSwitch.nextView);
            } else if (decision === "repair") {
              if (draftSwitch.kind === "raw") rawEditor.current?.focus();
              else if (draftSwitch.kind === "simple") {
                const panel = simplePanel.current;
                const field = panel?.querySelector<HTMLInputElement>(
                  "[data-profile-draft]:not([readonly]):not(:disabled)",
                );
                (field ?? panel)?.focus();
              } else {
                const panel = tablePanel.current;
                const field =
                  panel?.querySelector<HTMLInputElement>(
                    'input[aria-invalid="true"]:not([readonly])',
                  ) ??
                  panel?.querySelector<HTMLInputElement>(
                    "input[data-table-draft]:not([readonly])",
                  ) ??
                  panel?.querySelector<HTMLInputElement>(
                    "[data-table-filter-search]",
                  );
                (field ?? panel)?.focus();
              }
            }
            setDraftSwitch(undefined);
          }}
        />
      )}

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
            onClick={discardProfileDraft}
          >
            Discard draft
          </button>
          {simpleVisible && (
            <button
              className="editor-button editor-primary"
              type="button"
              disabled={busy || staleProfile}
              onClick={profileDraft.preview ? applyProfile : previewProfile}
            >
              {profileDraft.preview ? "Apply preview" : "Preview edits"}
            </button>
          )}
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
            onClick={discardDraft}
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
          {status}
          {dirtyStatus && (
            <span className="editor-dirty-status">{dirtyStatus}</span>
          )}
          {document.signatureStatus && <span>{document.signatureStatus}</span>}
          {controller.backupStatus && <span>{controller.backupStatus}</span>}
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
