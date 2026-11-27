import { useMemo, useState } from "react";
import type {
  EngineDiff,
  EngineOccurrenceDiff,
  EngineRevertTarget,
} from "../engine/protocol";
import { buttonClasses } from "../shared/control-styles";
import { EditorDialog } from "./editor-dialog";
import type { EditorSection } from "./editor-model";

const CHANGE_PAGE_SIZE = 100;
const OCCURRENCE_DEPTH = 3;

type Snapshot = Extract<
  EngineOccurrenceDiff["change"],
  { kind: "changed" }
>["payload"]["before"];

function describe(snapshot: Pick<Snapshot, "name">) {
  return snapshot.name || "Unnamed";
}

interface OccurrenceLocation {
  name: string;
  parent: string | null;
  position?: string;
}

function indexSiblings(
  locations: Map<string, OccurrenceLocation>,
  siblings: readonly { id: string; name: string }[],
  parent: string | null,
) {
  const byName = new Map<string, string[]>();
  for (const sibling of siblings) {
    const name = describe(sibling);
    const ids = byName.get(name);
    if (ids) ids.push(sibling.id);
    else byName.set(name, [sibling.id]);
  }
  for (const [name, ids] of byName) {
    for (const [index, id] of ids.entries()) {
      locations.set(id, {
        name,
        parent,
        position: ids.length > 1 ? `${index + 1} of ${ids.length}` : undefined,
      });
    }
  }
}

function indexCurrentLocations(sections: readonly EditorSection[]) {
  const locations = new Map<string, OccurrenceLocation>();
  indexSiblings(locations, sections, null);
  for (const section of sections) {
    indexSiblings(locations, section.keys ?? [], section.id);
    const valuesByKey = new Map<string, { id: string; name: string }[]>();
    for (const row of section.rows) {
      if (row.keyId === undefined || row.id === row.keyId) continue;
      const value = { id: row.id, name: row.childName ?? "Value" };
      const siblings = valuesByKey.get(row.keyId);
      if (siblings) siblings.push(value);
      else valuesByKey.set(row.keyId, [value]);
    }
    for (const [key, siblings] of valuesByKey)
      indexSiblings(locations, siblings, key);
  }
  return locations;
}

function indexOpeningLocations(diff: EngineDiff) {
  const opening = new Map<string, OccurrenceLocation>();
  for (const { id, change } of diff.changes) {
    if (change.kind === "added") continue;
    const snapshot =
      change.kind === "removed" ? change.payload : change.payload.before;
    opening.set(id, { name: describe(snapshot), parent: snapshot.parent });
  }
  return opening;
}

function occurrencePath(
  entry: EngineOccurrenceDiff,
  current: ReadonlyMap<string, OccurrenceLocation>,
  opening: ReadonlyMap<string, OccurrenceLocation>,
) {
  const { change } = entry;
  const snapshot =
    change.kind === "changed" ? change.payload.after : change.payload;
  const removed = change.kind === "removed";
  const resolve = (id: string) =>
    (removed ? opening.get(id) : undefined) ?? current.get(id);
  let component: OccurrenceLocation | undefined = resolve(entry.id) ?? {
    name: describe(snapshot),
    parent: snapshot.parent,
  };
  const path = [];
  for (let depth = 0; component && depth < OCCURRENCE_DEPTH; depth += 1) {
    path.push(
      component.position && !removed
        ? `${component.name} (${component.position})`
        : component.name,
    );
    component =
      component.parent === null ? undefined : resolve(component.parent);
  }
  return path.reverse().join(" › ");
}

function OccurrenceValue({
  snapshot,
  name = describe(snapshot),
}: {
  snapshot: Snapshot;
  name?: string;
}) {
  return (
    <>
      <span>{name}</span>
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
  name,
  disabled,
  onRevert,
}: {
  entry: EngineOccurrenceDiff;
  name: string;
  disabled: boolean;
  onRevert?: (target: EngineRevertTarget) => void;
}) {
  const { change, kind, id } = entry;

  if (change.kind !== "changed") {
    return (
      <li className="border-t border-line py-[0.4rem] wrap-anywhere">
        <p className="editor-diff-label font-semibold">
          {change.kind === "added" ? "Added" : "Removed"} {kind}
        </p>
        <OccurrenceValue snapshot={change.payload} name={name} />
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
  sections,
  busy,
  notice,
  onRevert,
  onDismiss,
}: {
  diff: EngineDiff;
  sections: readonly EditorSection[];
  busy: boolean;
  notice?: string;
  onRevert?: (target: EngineRevertTarget) => void;
  onDismiss: () => void;
}) {
  const [visible, setVisible] = useState(CHANGE_PAGE_SIZE);
  const current = useMemo(() => indexCurrentLocations(sections), [sections]);
  const opening = useMemo(() => indexOpeningLocations(diff), [diff]);

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
                name={occurrencePath(entry, current, opening)}
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
