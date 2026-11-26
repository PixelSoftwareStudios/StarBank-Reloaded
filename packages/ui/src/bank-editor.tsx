import { Tabs } from "@heroui/react/tabs";
import type { EngineDiff } from "@starbank/engine-client/protocol";
import type { AppSettings } from "@starbank/platform/settings";
import { lazy, Suspense, useEffect, useState } from "react";
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
  type EditorDraft,
  type EditorRow,
  prepareTableDraft,
} from "./editor-model";
import { TableEditor } from "./table-editor";

export type EditorSettings = Pick<
  AppSettings,
  "sectionsLayout" | "wrapRaw" | "groupNumbers"
>;

let rawEditorModule: Promise<typeof import("./raw-editor")> | undefined;

export function preloadRawEditor() {
  rawEditorModule ??= import("./raw-editor");
  return rawEditorModule;
}

const RawEditor = lazy(preloadRawEditor);
const StructureEditor = lazy(() => import("./structure-editor"));
const EditorDiff = lazy(() => import("./editor-diff"));

export function BankEditor({
  controller,
  document,
  busy,
  notice,
  clearNotice,
  run,
  settings,
  onOpenSettings,
}: {
  controller: BankEditorController;
  document: BankEditorDocument;
  busy: boolean;
  notice?: string;
  clearNotice: () => void;
  run: RunEditorAction;
  settings?: EditorSettings;
  onOpenSettings?: () => void;
}) {
  const [draft, setDraft] = useState<EditorDraft>();
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
  const hasDraft = draft !== undefined;
  const pending = busy || hasDraft;
  useEffect(() => onDraftChange?.(hasDraft), [hasDraft, onDraftChange]);

  function stageValues(rows: readonly EditorRow[], value: string) {
    setDraft((current) => {
      if (
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
    notice || controller.status || (document.dirty ? "Unsaved changes" : "");

  return (
    <div
      className="document-editor"
      {...fileDropHandlers(controller.onOpenFile, pending, run)}
    >
      <header className="editor-header">
        <div className="editor-open-actions">
          <OpenBankButton
            controller={controller}
            disabled={pending}
            run={run}
          />
          <SettingsButton open={onOpenSettings} />
        </div>
        <h1 title={document.name}>{document.name}</h1>
      </header>

      <Tabs
        className="editor-views"
        defaultSelectedKey="table"
        variant="secondary"
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label="Bank views">
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
        <Tabs.Panel id="table" className="editor-view">
          <TableEditor
            sections={document.sections}
            values={tableDraft?.values}
            invalid={prepared?.invalid}
            canEdit={Boolean(onEditValues) && !busy && !stale && !rawDraft}
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
                !onReplaceXml || busy || stale || tableDraft !== undefined
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
