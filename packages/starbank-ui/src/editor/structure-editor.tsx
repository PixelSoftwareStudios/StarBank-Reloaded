import { type FormEvent, useId, useLayoutEffect, useState } from "react";
import {
  type EngineStructureChange,
  type EngineStructuredValueKind,
  STRUCTURED_VALUE_KINDS,
} from "../engine/protocol";
import { buttonClasses, focusOutlineClasses } from "../shared/control-styles";
import { EditorDialog } from "./editor-dialog";
import type { EditorRow, EditorSection } from "./index";

type Operation = EngineStructureChange["kind"];
type Scope = "bank" | "section" | "key" | "value";
type Fields = "name" | "value" | "name_and_value" | "none";

interface OperationOption {
  kind: Operation;
  label: string;
  target: Scope;
  fields: Fields;
}

const operations: readonly OperationOption[] = [
  { kind: "add_section", label: "Add section", target: "bank", fields: "name" },
  {
    kind: "rename_section",
    label: "Rename section",
    target: "section",
    fields: "name",
  },
  {
    kind: "remove_section",
    label: "Delete section",
    target: "section",
    fields: "none",
  },
  { kind: "add_key", label: "Add key", target: "section", fields: "name" },
  { kind: "rename_key", label: "Rename key", target: "key", fields: "name" },
  { kind: "remove_key", label: "Delete key", target: "key", fields: "none" },
  {
    kind: "add_value",
    label: "Add value",
    target: "key",
    fields: "name_and_value",
  },
  {
    kind: "rename_value",
    label: "Rename value",
    target: "value",
    fields: "name",
  },
  {
    kind: "set_value_type",
    label: "Change value type",
    target: "value",
    fields: "value",
  },
  {
    kind: "remove_value",
    label: "Delete value",
    target: "value",
    fields: "none",
  },
];

interface Choice {
  id: string;
  name: string;
}

export interface StructureDraft {
  readonly name: string;
  readonly valueKind: EngineStructuredValueKind;
  readonly value: string;
}

const fieldLabelClasses = "grid gap-[0.15rem]";
const fieldClasses = `min-h-7.5 w-full min-w-0 rounded-sm border border-line bg-field px-[0.4rem] py-[0.2rem] ${focusOutlineClasses}`;

function OccurrenceSelect({
  label,
  choices,
  current,
  disabled,
  onSelect,
}: {
  label: string;
  choices: readonly Choice[];
  current?: string;
  disabled: boolean;
  onSelect: (id: string) => void;
}) {
  const labelId = useId();
  const counts = new Map<string, number>();
  const seen = new Map<string, number>();
  for (const { name } of choices) counts.set(name, (counts.get(name) ?? 0) + 1);

  return (
    <label className={fieldLabelClasses}>
      <span id={labelId}>{label}</span>
      <select
        className={fieldClasses}
        aria-labelledby={labelId}
        disabled={disabled || choices.length === 0}
        value={current ?? ""}
        onChange={(event) => onSelect(event.currentTarget.value)}
      >
        {choices.length === 0 && <option value="">None available</option>}
        {choices.map(({ id, name }) => {
          const count = counts.get(name) ?? 0;
          const index = (seen.get(name) ?? 0) + 1;
          seen.set(name, index);
          const shown = name || "Unnamed";
          return (
            <option key={id} value={id}>
              {count > 1 ? `${shown} (${index} of ${count})` : shown}
            </option>
          );
        })}
      </select>
    </label>
  );
}

function buildChange(
  kind: Operation,
  id: string,
  name: string,
  value_kind: EngineStructuredValueKind,
  value: string,
): EngineStructureChange {
  switch (kind) {
    case "add_section":
      return { kind, name };
    case "add_key":
      return { kind, section_id: id, name };
    case "add_value":
      return { kind, key_id: id, name, value_kind, value };
    case "set_value_type":
      return { kind, id, value_kind, value };
    case "rename_section":
    case "rename_key":
    case "rename_value":
      return { kind, id, name };
    case "remove_section":
    case "remove_key":
    case "remove_value":
      return { kind, id };
  }
}

function StructureForm({
  operation,
  target,
  row,
  disabled,
  onApply,
  onDraftChange,
}: {
  operation: OperationOption;
  target?: Choice;
  row?: EditorRow;
  disabled: boolean;
  onApply: (change: EngineStructureChange) => void;
  onDraftChange: (draft?: StructureDraft) => void;
}) {
  const { kind, fields } = operation;
  const typeLabel = useId();
  const initial: StructureDraft = {
    name: kind.startsWith("rename_")
      ? (target?.name ?? "")
      : kind === "add_value"
        ? "Value"
        : "",
    valueKind:
      STRUCTURED_VALUE_KINDS.find((item) => item === row?.type) ?? "string",
    value: row?.value ?? "",
  };
  const [draft, setDraft] = useState(initial);
  const editsName = fields === "name" || fields === "name_and_value";
  const editsValue = fields === "value" || fields === "name_and_value";
  const changed =
    (editsName && draft.name !== initial.name) ||
    (editsValue &&
      (draft.valueKind !== initial.valueKind || draft.value !== initial.value));
  const removes = kind.startsWith("remove_");
  const available = operation.target === "bank" || target !== undefined;

  useLayoutEffect(
    () => onDraftChange(changed ? draft : undefined),
    [changed, draft, onDraftChange],
  );

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (disabled || !available) return;
    if (
      removes &&
      !window.confirm(`${operation.label} "${target?.name || "Unnamed"}"?`)
    )
      return;
    onApply(
      buildChange(
        kind,
        target?.id ?? "",
        draft.name,
        draft.valueKind,
        draft.value,
      ),
    );
  }

  return (
    <form
      className="editor-dialog-fields mt-[0.4rem] grid gap-[0.4rem]"
      onSubmit={submit}
    >
      {editsName && (
        <label className={fieldLabelClasses}>
          {kind === "add_value" || kind === "rename_value"
            ? "Element name"
            : "Name"}
          <input
            className={fieldClasses}
            disabled={disabled}
            value={draft.name}
            onChange={(event) =>
              setDraft({ ...draft, name: event.currentTarget.value })
            }
            required={kind === "add_value" || kind === "rename_value"}
          />
        </label>
      )}
      {editsValue && (
        <>
          <label className={fieldLabelClasses}>
            <span id={typeLabel}>Value type</span>
            <select
              className={fieldClasses}
              aria-labelledby={typeLabel}
              disabled={disabled}
              value={draft.valueKind}
              onChange={(event) => {
                const next = STRUCTURED_VALUE_KINDS.find(
                  (item) => item === event.currentTarget.value,
                );
                if (next) setDraft({ ...draft, valueKind: next });
              }}
            >
              {STRUCTURED_VALUE_KINDS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <label className={fieldLabelClasses}>
            Value
            <input
              className={fieldClasses}
              disabled={disabled}
              value={draft.value}
              onChange={(event) =>
                setDraft({ ...draft, value: event.currentTarget.value })
              }
            />
          </label>
        </>
      )}
      <button
        className={`${buttonClasses("primary")} editor-structure-submit justify-self-end`}
        type="submit"
        disabled={disabled || !available}
      >
        {operation.label}
      </button>
    </form>
  );
}

export default function StructureEditor({
  sections,
  initialSection,
  busy,
  notice,
  onApply,
  onDraftChange,
  onDismiss,
}: {
  sections: readonly EditorSection[];
  initialSection?: string;
  busy: boolean;
  notice?: string;
  onApply: (change: EngineStructureChange) => void;
  onDraftChange: (draft?: StructureDraft) => void;
  onDismiss: () => void;
}) {
  const actionLabel = useId();
  const [operation, setOperation] = useState<OperationOption>(operations[0]);
  const [sectionId, setSectionId] = useState(initialSection);
  const [keyId, setKeyId] = useState<string>();
  const [valueId, setValueId] = useState<string>();
  const section = sections.find(({ id }) => id === sectionId) ?? sections[0];
  const keys = section?.keys ?? [];
  const key = keys.find(({ id }) => id === keyId) ?? keys[0];
  const rows = key
    ? (section?.rows.filter((row) => row.keyId === key.id) ?? [])
    : [];
  const row = rows.find(({ id }) => id === valueId) ?? rows[0];
  const values = rows.map(({ id, childName }) => ({
    id,
    name: childName || "Value",
  }));
  const choice = values.find(({ id }) => id === row?.id);
  const target =
    operation.target === "section"
      ? section
      : operation.target === "key"
        ? key
        : operation.target === "value"
          ? choice
          : undefined;
  const needsKeys = operation.target === "key" || operation.target === "value";

  return (
    <EditorDialog title="Edit bank structure" onDismiss={onDismiss}>
      {notice && (
        <p
          className="editor-dialog-notice my-[0.4rem] text-[color:var(--editor-danger)]"
          role="status"
        >
          {notice}
        </p>
      )}
      <div className="editor-dialog-fields grid gap-[0.4rem]">
        <label className={fieldLabelClasses}>
          <span id={actionLabel}>Action</span>
          <select
            className={fieldClasses}
            aria-labelledby={actionLabel}
            disabled={busy}
            value={operation.kind}
            onChange={(event) => {
              const next = operations.find(
                ({ kind }) => kind === event.currentTarget.value,
              );
              if (next) setOperation(next);
            }}
          >
            {operations.map(({ kind, label }) => (
              <option key={kind} value={kind}>
                {label}
              </option>
            ))}
          </select>
        </label>
        {operation.target !== "bank" && (
          <OccurrenceSelect
            label="Section"
            choices={sections}
            current={section?.id}
            disabled={busy}
            onSelect={setSectionId}
          />
        )}
        {needsKeys && (
          <OccurrenceSelect
            label="Key"
            choices={keys}
            current={key?.id}
            disabled={busy}
            onSelect={setKeyId}
          />
        )}
        {operation.target === "value" && (
          <OccurrenceSelect
            label="Value element"
            choices={values}
            current={row?.id}
            disabled={busy}
            onSelect={setValueId}
          />
        )}
      </div>
      <StructureForm
        key={`${operation.kind}:${target?.id ?? ""}`}
        operation={operation}
        target={target}
        row={operation.target === "value" ? row : undefined}
        disabled={busy}
        onApply={onApply}
        onDraftChange={onDraftChange}
      />
    </EditorDialog>
  );
}
