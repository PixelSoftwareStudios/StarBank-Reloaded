import { useState } from "react";
import type {
  EngineDiff,
  EngineOccurrenceDiff,
  EngineRevertTarget,
} from "../engine/protocol";
import { buttonClasses } from "../shared/control-styles";
import { EditorDialog } from "./editor-dialog";

const CHANGE_PAGE_SIZE = 100;

type Snapshot = Extract<
  EngineOccurrenceDiff["change"],
  { kind: "changed" }
>["payload"]["before"];

function describe(snapshot: Snapshot) {
  return snapshot.name || "Unnamed";
}

function OccurrenceValue({ snapshot }: { snapshot: Snapshot }) {
  return (
    <>
      <span>{describe(snapshot)}</span>
      {snapshot.value && (
        <>
          <span className="editor-diff-kind mx-[0.35rem] text-muted">
            {snapshot.value.kind}
          </span>
          <code className="whitespace-pre-wrap">
            {snapshot.value.text || '""'}
          </code>
        </>
      )}
    </>
  );
}

function ChangeRow({
  entry,
  disabled,
  onRevert,
}: {
  entry: EngineOccurrenceDiff;
  disabled: boolean;
  onRevert?: (target: EngineRevertTarget) => void;
}) {
  const { change, kind, id } = entry;
  const name = describe(
    change.kind === "changed" ? change.payload.after : change.payload,
  );

  if (change.kind !== "changed") {
    return (
      <li className="border-t border-line py-[0.4rem] wrap-anywhere">
        <p className="editor-diff-label font-semibold">
          {change.kind === "added" ? "Added" : "Removed"} {kind}
        </p>
        <OccurrenceValue snapshot={change.payload} />
      </li>
    );
  }

  const { before, after } = change.payload;
  const describeSides =
    before.name !== after.name ||
    before.value?.kind !== after.value?.kind ||
    before.value?.text !== after.value?.text;

  return (
    <li className="border-t border-line py-[0.4rem] wrap-anywhere">
      <div className="editor-diff-row flex items-center justify-between gap-2">
        <p className="editor-diff-label font-semibold">
          Changed {kind} {name}
        </p>
        {onRevert && (
          <button
            className={buttonClasses()}
            type="button"
            disabled={disabled}
            aria-label={`Revert ${kind} ${name}`}
            onClick={() => onRevert({ kind, id })}
          >
            Revert
          </button>
        )}
      </div>
      {describeSides && (
        <dl className="editor-diff-values grid grid-cols-[auto_minmax(0,1fr)] gap-x-2 gap-y-[0.15rem]">
          <dt>Opening</dt>
          <dd>
            <OccurrenceValue snapshot={before} />
          </dd>
          <dt>Current</dt>
          <dd>
            <OccurrenceValue snapshot={after} />
          </dd>
        </dl>
      )}
    </li>
  );
}

export default function EditorDiff({
  diff,
  busy,
  notice,
  onRevert,
  onDismiss,
}: {
  diff: EngineDiff;
  busy: boolean;
  notice?: string;
  onRevert?: (target: EngineRevertTarget) => void;
  onDismiss: () => void;
}) {
  const [visible, setVisible] = useState(CHANGE_PAGE_SIZE);

  return (
    <EditorDialog title="Changes since opening" onDismiss={onDismiss}>
      {notice && (
        <p
          className="editor-dialog-notice my-[0.4rem] text-[color:var(--editor-danger)]"
          role="status"
        >
          {notice}
        </p>
      )}
      {diff.changes.length > 0 ? (
        <>
          <ul className="editor-diff-list">
            {diff.changes.slice(0, visible).map((entry) => (
              <ChangeRow
                key={entry.id}
                entry={entry}
                disabled={busy}
                onRevert={onRevert}
              />
            ))}
          </ul>
          {visible < diff.changes.length && (
            <button
              className={buttonClasses()}
              type="button"
              onClick={() => setVisible(visible + CHANGE_PAGE_SIZE)}
            >
              Show more changes
            </button>
          )}
        </>
      ) : (
        <p>
          {diff.source_changed
            ? "The XML changed outside the Table View."
            : "No changes since opening."}
        </p>
      )}
    </EditorDialog>
  );
}
