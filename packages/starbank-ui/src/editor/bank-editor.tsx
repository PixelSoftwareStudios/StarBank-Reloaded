import * as Tabs from "@radix-ui/react-tabs";
import {
  lazy,
  type ReactNode,
  Suspense,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { EngineDiff } from "../engine/protocol";
import { type MapContext, MapContextDetails } from "../inventory/map-context";
import type { AppSettings } from "../settings/settings";
import { buttonClasses, focusOutlineClasses } from "../shared/control-styles";
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
import {
  type BankEditorController,
  type BankEditorDocument,
  type EditorAction,
  type EditorActionResult,
  type EditorDraft,
  type EditorProfileField,
  type EditorProfileInput,
  type EditorRow,
  profileStatus,
  useEditorState,
} from "./index";
import type { RawEditorHandle } from "./raw-editor";
import { ProfilePicker, SimpleEditor } from "./simple-editor";
import {
  type TableValidationRequest,
  useTableDraftValidation,
} from "./table-draft-validation";
import { TableEditor, type TableEditorHandle } from "./table-editor";

type EditorView = NonNullable<ReturnType<typeof useEditorState>["view"]>;

const tabClasses = `editor-tab min-h-[var(--control-height)] border-b-2 border-transparent px-4 whitespace-nowrap text-muted data-[state=active]:border-focus data-[state=active]:font-semibold data-[state=active]:text-ink ${focusOutlineClasses}`;
const viewClasses = "editor-view m-0 h-full min-h-0 min-w-0 overflow-auto p-0";
const draftStatusClasses = "min-w-0 flex-[1_1_8rem] text-muted wrap-anywhere";
const statusMarkerClasses =
  "text-[0.75rem] before:ms-2 before:me-1.5 before:inline-block before:size-1 before:rounded-full before:align-[0.15em] before:content-['']";

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

function labelDraftFields(
  sections: BankEditorDocument["sections"],
  values: ReadonlyMap<string, string>,
) {
  const labels = new Map<string, { section: string; field: string }>();
  for (const section of sections) {
    for (const row of section.rows) {
      if (values.has(row.id)) {
        labels.set(row.id, {
          section: section.name,
          field: row.childName ? `${row.name} · ${row.childName}` : row.name,
        });
      }
    }
  }
  return labels;
}

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
  const {
    draft,
    profileDraft,
    view,
    getDraft,
    setDraft,
    setProfileDraft,
    setView,
  } = useEditorState();
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
  const [retainedSimpleProfile, setRetainedSimpleProfile] = useState<string>();
  const tablePanel = useRef<HTMLDivElement>(null);
  const tableEditor = useRef<TableEditorHandle>(null);
  const [retainTable, setRetainTable] = useState(false);
  const rawEditor = useRef<RawEditorHandle>(null);
  const [retainedRawRevision, setRetainedRawRevision] = useState<string>();
  const [rawRestart, setRawRestart] = useState(false);
  const [draftLabels, setDraftLabels] = useState<{
    revision: string;
    fields: ReturnType<typeof labelDraftFields>;
  }>();
  const target = { id: document.id, revision: document.revision };
  const mounted = useRef(false);
  useLayoutEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const tableDraft = draft?.kind === "table" ? draft : undefined;
  const rawDraft = draft?.kind === "raw" ? draft : undefined;
  const stale = draft !== undefined && draft.revision !== document.revision;
  const {
    onEditValues,
    onValidateValues,
    onReplaceXml,
    onDraftChange,
    onRefreshSnapshot,
    onStructure,
    onDiff,
    onRevert,
  } = controller;
  const snapshotNeedsRefresh =
    onRefreshSnapshot !== undefined || controller.snapshotUnavailable === true;
  const editsBlocked = busy || snapshotNeedsRefresh;
  const hasValidator = onValidateValues !== undefined;
  const tableEdits = useMemo(
    () =>
      tableDraft &&
      Array.from(tableDraft.values, ([id, value]) => ({ id, value })),
    [tableDraft],
  );
  const validationRequest = useMemo(
    () =>
      hasValidator && !stale && tableEdits?.length
        ? {
            target: { id: document.id, revision: document.revision },
            edits: tableEdits,
          }
        : undefined,
    [document.id, document.revision, hasValidator, tableEdits, stale],
  );
  const validation = useTableDraftValidation(
    validationRequest,
    onValidateValues,
    editsBlocked,
  );
  const validationPending = validation?.kind === "pending";
  const tableIssues =
    validation?.kind === "checked" ? validation.issues : undefined;
  const hasInvalidTableEdits = Boolean(tableIssues?.size);
  const [waitingTableSwitch, setWaitingTableSwitch] = useState<{
    request: TableValidationRequest;
    nextView: EditorView;
  }>();
  const hasDraft = draft !== undefined || profileDraft !== undefined;
  const pending = editsBlocked || hasDraft;
  const outputDisabled =
    editsBlocked ||
    Boolean(profileDraft) ||
    stale ||
    (tableDraft !== undefined && (!onEditValues || hasInvalidTableEdits)) ||
    (rawDraft !== undefined && !onReplaceXml);
  useLayoutEffect(() => {
    onDraftChange?.(draft !== undefined || profileDraft !== undefined);
  }, [draft, profileDraft, onDraftChange]);

  const simpleVisible = selectedView === "simple";
  const staleProfile =
    profileDraft !== undefined &&
    (profileDraft.revision !== document.revision ||
      profileDraft.profile_id !== profile?.profile_id);

  function stageProfile(field: EditorProfileField, input: EditorProfileInput) {
    if (!profile || editsBlocked || draft || staleProfile || field.read_only)
      return;
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
      editsBlocked
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
      editsBlocked
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
    if (simpleVisible && profile) setRetainedSimpleProfile(profile.profile_id);
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
    if (
      !tableEdits ||
      hasInvalidTableEdits ||
      validationPending ||
      !onEditValues
    )
      return;
    if (tableEdits.length > 0)
      submitDraft("table", () => onEditValues(target, tableEdits), nextView);
    else finishDraft(nextView);
  }

  function apply() {
    if (editsBlocked || stale) return;
    if (rawDraft && onReplaceXml)
      submitDraft("raw", () => onReplaceXml(target, rawDraft.text));
    else if (tableDraft) applyTable();
  }

  async function saveDraft(action: EditorAction): Promise<EditorActionResult> {
    const submitted = draft;
    const changed = {
      ok: false,
      message: "The bank changed while you were editing this draft.",
    } as const;
    if (outputDisabled || getDraft() !== submitted) return changed;
    if (!submitted) return action(target);

    const result =
      submitted.kind === "raw"
        ? await onReplaceXml?.(target, submitted.text)
        : tableEdits && (await onEditValues?.(target, tableEdits));
    if (!result) return changed;
    if (!result.ok) return result;
    if (
      !mounted.current ||
      result.target.id !== target.id ||
      getDraft() !== submitted
    ) {
      return changed;
    }
    setDraft(undefined);
    const output = await action(result.target);
    return output.ok && !output.message
      ? { ...output, message: result.message }
      : output;
  }

  function discardDraft() {
    setDraft(undefined);
    clearNotice();
    if (rawDraft) {
      setRetainedRawRevision(undefined);
      setRawRestart((previous) => !previous);
    }
  }

  function refreshEditor() {
    if (!onRefreshSnapshot) return;
    const current = getDraft();
    if (current?.kind === "table" && current.revision === document.revision) {
      setDraftLabels({
        revision: current.revision,
        fields: labelDraftFields(document.sections, current.values),
      });
    }
    run(() => onRefreshSnapshot(target));
  }

  function selectView(nextView: EditorView) {
    if (snapshotNeedsRefresh) return;
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
    } else if (selectedView === "table" && tableEdits) {
      if (validationPending && validationRequest && onEditValues) {
        setWaitingTableSwitch({ request: validationRequest, nextView });
        return;
      }
      const message = stale
        ? staleMessage
        : !onEditValues
          ? "The Table draft cannot currently be applied."
          : tableIssues?.values().next().value;
      if (message) setDraftSwitch({ kind: "table", nextView, message });
      else applyTable(nextView);
    } else {
      showView(nextView);
    }
  }

  const finishTableSwitch = useEffectEvent(selectView);
  useEffect(() => {
    if (!waitingTableSwitch) return;
    const matching = waitingTableSwitch.request === validationRequest;
    if (matching && (validationPending || busy)) return;
    setWaitingTableSwitch(undefined);
    if (matching) finishTableSwitch(waitingTableSwitch.nextView);
  }, [busy, validationPending, validationRequest, waitingTableSwitch]);

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
  const tableReview = useMemo(() => {
    if (
      !tableDraft ||
      !tableEdits ||
      !(stale || controller.snapshotUnavailable)
    )
      return undefined;
    const labels =
      draftLabels?.revision === tableDraft.revision
        ? draftLabels.fields
        : labelDraftFields(document.sections, tableDraft.values);
    return JSON.stringify(
      tableEdits.map(({ id, value }) => ({
        ...(labels.get(id) ?? { id }),
        value,
      })),
      null,
      2,
    );
  }, [
    controller.snapshotUnavailable,
    document.sections,
    draftLabels,
    stale,
    tableDraft,
    tableEdits,
  ]);

  return (
    <div
      className="document-editor [--editor-primary-bg:var(--primary-action-bg)] [--editor-primary-text:var(--primary-action-text)] grid h-full min-h-[30rem] min-w-0 grid-rows-[auto_minmax(0,1fr)_auto_auto] gap-2 p-3.5 pb-[calc(0.875rem_+_2px)] text-[0.875rem] min-[42rem]:min-h-0 [.starbank-shell[data-density=comfortable]_&]:gap-3 [.starbank-shell[data-density=comfortable]_&]:p-[1.125rem] [.starbank-shell[data-density=comfortable]_&]:pb-[calc(1.125rem_+_2px)]"
      {...fileDropHandlers(controller.onOpenFile, pending, run)}
    >
      <header className="editor-header grid gap-[0.3125rem]">
        <div className="editor-header-bar flex min-w-0 items-center gap-1.5">
          {showBreadcrumbs && document.breadcrumbs && (
            <BankBreadcrumbs segments={document.breadcrumbs} />
          )}
          <div className="editor-open-actions ms-auto flex flex-none items-center justify-end gap-1.5">
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
        <h1
          className="mt-1 truncate text-center text-[1.75rem] leading-[1.2] font-semibold tracking-[-0.025em]"
          title={title}
        >
          {title}
        </h1>
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
        className="editor-views grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)]"
        value={selectedView}
        activationMode={hasDraft ? "manual" : "automatic"}
        onValueChange={(key) => {
          if (key === "simple" || key === "table" || key === "raw")
            selectView(key);
        }}
      >
        <Tabs.List
          aria-label="Bank views"
          className="editor-tab-list flex overflow-auto border-b border-line"
          onKeyDown={(event) => {
            // Radix activates the tab from keydown. Its native button click can
            // otherwise activate the newly focused dialog button.
            if (hasDraft && (event.key === "Enter" || event.key === " "))
              event.preventDefault();
          }}
        >
          {profile && (
            <Tabs.Trigger className={tabClasses} value="simple">
              Simple View
            </Tabs.Trigger>
          )}
          <Tabs.Trigger className={tabClasses} value="table">
            Table View
          </Tabs.Trigger>
          <Tabs.Trigger className={tabClasses} value="raw">
            Raw View
          </Tabs.Trigger>
        </Tabs.List>
        {profile && (
          <Tabs.Content
            key={profile.profile_id}
            value="simple"
            className={viewClasses}
            forceMount={
              simpleVisible || retainedSimpleProfile === profile.profile_id
                ? true
                : undefined
            }
            hidden={!simpleVisible}
            ref={simplePanel}
          >
            <SimpleEditor
              profile={profile}
              draft={profileDraft}
              disabled={editsBlocked || Boolean(draft) || staleProfile}
              onChange={stageProfile}
            />
          </Tabs.Content>
        )}
        <Tabs.Content
          value="table"
          className={viewClasses}
          forceMount={tableVisible || retainTable ? true : undefined}
          hidden={!tableVisible}
          ref={tablePanel}
        >
          <TableEditor
            ref={tableEditor}
            sections={document.sections}
            values={stale ? undefined : tableDraft?.values}
            issues={tableIssues}
            validationError={
              validation?.kind === "failed" ? validation.message : undefined
            }
            canEdit={
              Boolean(onEditValues) &&
              !editsBlocked &&
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
          className={viewClasses}
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
                editsBlocked ||
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
                const issueId = stale
                  ? undefined
                  : tableIssues?.keys().next().value;
                if (!issueId || !tableEditor.current?.repairIssue(issueId)) {
                  const panel = tablePanel.current;
                  const field =
                    (issueId === undefined && !stale
                      ? panel?.querySelector<HTMLInputElement>(
                          "input[data-table-draft]:not([readonly])",
                        )
                      : undefined) ??
                    panel?.querySelector<HTMLInputElement>(
                      "[data-table-filter-search]",
                    );
                  (field ?? panel)?.focus();
                }
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
            busy={editsBlocked}
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
            busy={editsBlocked}
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
        <div className="editor-draft-actions flex flex-wrap items-center gap-2 border-t border-line pt-2">
          <p className={draftStatusClasses} role="status">
            {staleProfile
              ? "The bank or profile changed. Discard this draft to continue."
              : "Unapplied Simple View changes"}
          </p>
          <button
            className={buttonClasses()}
            type="button"
            disabled={busy}
            onClick={discardProfileDraft}
          >
            Discard draft
          </button>
          {simpleVisible && (
            <button
              className={buttonClasses("primary")}
              type="button"
              disabled={editsBlocked || staleProfile}
              onClick={profileDraft.preview ? applyProfile : previewProfile}
            >
              {profileDraft.preview ? "Apply preview" : "Preview edits"}
            </button>
          )}
        </div>
      )}

      {draft && (
        <div className="editor-draft-actions flex flex-wrap items-center gap-2 border-t border-line pt-2">
          <p className={draftStatusClasses} role="status">
            {stale
              ? "The bank changed while you were editing. Discard this draft to continue."
              : draft.kind === "raw"
                ? "Unapplied Raw View changes"
                : "Unapplied Table View changes"}
          </p>
          <button
            className={buttonClasses()}
            type="button"
            disabled={busy}
            onClick={discardDraft}
          >
            Discard draft
          </button>
          <button
            className={buttonClasses("primary")}
            type="button"
            disabled={
              editsBlocked || stale || hasInvalidTableEdits || validationPending
            }
            onClick={apply}
          >
            {draft.kind === "raw" ? "Apply XML" : "Apply edits"}
          </button>
          {tableReview && (
            <details className="min-w-0 basis-full">
              <summary className="cursor-pointer">Review Table draft</summary>
              <textarea
                aria-label="Original Table draft"
                className="mt-2 w-full rounded-control border border-control-edge bg-field p-2 font-mono"
                readOnly
                rows={4}
                value={tableReview}
              />
            </details>
          )}
        </div>
      )}

      <footer className="editor-footer row-start-4 justify-between flex flex-wrap items-center gap-2 border-t border-line pt-2">
        <div
          className="editor-status min-w-0 flex-[1_1_8rem] text-muted wrap-anywhere"
          role="status"
        >
          {status}
          {dirtyStatus && (
            <span
              className={`editor-dirty-status ${statusMarkerClasses} before:bg-[var(--editor-unsaved)]`}
            >
              {dirtyStatus}
            </span>
          )}
          {document.signatureStatus && (
            <span className={`${statusMarkerClasses} before:bg-current`}>
              {document.signatureStatus}
            </span>
          )}
          {controller.backupStatus && (
            <span className={`${statusMarkerClasses} before:bg-current`}>
              {controller.backupStatus}
            </span>
          )}
        </div>
        {onRefreshSnapshot && (
          <button
            className={buttonClasses()}
            type="button"
            disabled={busy}
            onClick={refreshEditor}
          >
            Refresh editor
          </button>
        )}
        {controller.onRecoveryCopy && (
          <button
            className={buttonClasses()}
            type="button"
            disabled={busy}
            onClick={() => {
              const copy = controller.onRecoveryCopy;
              if (copy) run(() => copy(target));
            }}
          >
            Download recovery copy…
          </button>
        )}
        <EditorActions
          controller={controller}
          target={target}
          disabled={pending}
          outputDisabled={outputDisabled}
          saveDraft={saveDraft}
          run={run}
        />
      </footer>
    </div>
  );
}
