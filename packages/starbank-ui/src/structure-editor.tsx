import { type FormEvent, useId, useState } from "react";
import {
  type EngineStructureChange,
  type EngineStructuredValueKind,
  STRUCTURED_VALUE_KINDS,
} from "starbank-engine-client/protocol";
import { EditorDialog } from "./editor-dialog";
import type { EditorRow, EditorSection } from "./editor-model";

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

function OccurrenceSelect({
  label,
  choices,
  current,
  onSelect,
}: {
  label: string;
  choices: readonly Choice[];
  current?: string;
  onSelect: (id: string) => void;
}) {
  const labelId = useId();
  const counts = new Map<string, number>();
  const seen = new Map<string, number>();
  for (const { name } of choices) counts.set(name, (counts.get(name) ?? 0) + 1);

  return (
    <label>
      <span id={labelId}>{label}</span>
      <select
        aria-labelledby={labelId}
        disabled={choices.length === 0}
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
}: {
  operation: OperationOption;
  target?: Choice;
  row?: EditorRow;
  disabled: boolean;
  onApply: (change: EngineStructureChange) => void;
}) {
  const { kind, fields } = operation;
  const typeLabel = useId();
  const [name, setName] = useState(
    kind.startsWith("rename_")
      ? (target?.name ?? "")
      : kind === "add_value"
        ? "Value"
        : "",
  );
  const [valueKind, setValueKind] = useState<EngineStructuredValueKind>(
    STRUCTURED_VALUE_KINDS.find((item) => item === row?.type) ?? "string",
  );
  const [value, setValue] = useState(row?.value ?? "");
  const removes = kind.startsWith("remove_");
  const available = operation.target === "bank" || target !== undefined;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (disabled || !available) return;
    if (
      removes &&
      !window.confirm(`${operation.label} "${target?.name || "Unnamed"}"?`)
    )
      return;
    onApply(buildChange(kind, target?.id ?? "", name, valueKind, value));
  }

  return (
    <form className="editor-dialog-fields" onSubmit={submit}>
      {(fields === "name" || fields === "name_and_value") && (
        <label>
          {kind === "add_value" || kind === "rename_value"
            ? "Element name"
            : "Name"}
          <input
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
            required={kind === "add_value" || kind === "rename_value"}
          />
        </label>
      )}
      {(fields === "value" || fields === "name_and_value") && (
        <>
          <label>
            <span id={typeLabel}>Value type</span>
            <select
              aria-labelledby={typeLabel}
              value={valueKind}
              onChange={(event) => {
                const next = STRUCTURED_VALUE_KINDS.find(
                  (item) => item === event.currentTarget.value,
                );
                if (next) setValueKind(next);
              }}
            >
              {STRUCTURED_VALUE_KINDS.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>
          <label>
            Value
            <input
              value={value}
              onChange={(event) => setValue(event.currentTarget.value)}
            />
          </label>
        </>
      )}
      <button
        className="editor-button editor-primary editor-structure-submit"
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
  onDismiss,
}: {
  sections: readonly EditorSection[];
  initialSection?: string;
  busy: boolean;
  notice?: string;
  onApply: (change: EngineStructureChange) => void;
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
        <p className="editor-dialog-notice" role="status">
          {notice}
        </p>
      )}
      <div className="editor-dialog-fields">
        <label>
          <span id={actionLabel}>Action</span>
          <select
            aria-labelledby={actionLabel}
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
            onSelect={setSectionId}
          />
        )}
        {needsKeys && (
          <OccurrenceSelect
            label="Key"
            choices={keys}
            current={key?.id}
            onSelect={setKeyId}
          />
        )}
        {operation.target === "value" && (
          <OccurrenceSelect
            label="Value element"
            choices={values}
            current={row?.id}
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
      />
    </EditorDialog>
  );
}
