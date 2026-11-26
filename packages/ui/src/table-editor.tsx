import { useId, useState } from "react";
import type { EditorRow, EditorSection } from "./editor-model";

const NUMERIC_GROUP_SIZE = 3;

function groupNumberLexeme(type: string, value: string): string {
  if (
    (type !== "int" && type !== "fixed") ||
    !/^-?[0-9]+(?:\.[0-9]+)?$/.test(value)
  ) {
    return value;
  }
  const negative = value.startsWith("-");
  const [integer = "", fraction] = (negative ? value.slice(1) : value).split(
    ".",
  );
  if (integer.length <= NUMERIC_GROUP_SIZE) return value;
  const groups = [];
  for (let end = integer.length; end > 0; end -= NUMERIC_GROUP_SIZE) {
    groups.push(integer.slice(Math.max(0, end - NUMERIC_GROUP_SIZE), end));
  }
  const sign = negative ? "-" : "";
  const decimals = fraction === undefined ? "" : `.${fraction}`;
  return sign + groups.reverse().join(",") + decimals;
}

function rowLabel(row: EditorRow) {
  return row.childName ? `${row.name} · ${row.childName}` : row.name;
}

export function TableEditor({
  sections,
  values,
  invalid,
  canEdit,
  onChange,
  layout = "separate",
  groupNumbers = false,
}: {
  sections: readonly EditorSection[];
  values?: ReadonlyMap<string, string>;
  invalid?: ReadonlySet<string>;
  canEdit: boolean;
  onChange: (rows: readonly EditorRow[], value: string) => void;
  layout?: "separate" | "unified";
  groupNumbers?: boolean;
}) {
  const instructionsId = useId();
  const [sectionId, setSectionId] = useState<string>();
  const [selectedRows, setSelectedRows] = useState<ReadonlySet<string>>(
    new Set(),
  );
  const [bulkValue, setBulkValue] = useState("");
  const [editing, setEditing] = useState<string>();
  const section = sections.find(({ id }) => id === sectionId) ?? sections[0];
  const unified = layout === "unified";
  const visible = unified ? sections : section ? [section] : [];
  const visibleRows = visible.flatMap((item) =>
    item.rows.map((row) => ({ row, sectionName: item.name })),
  );
  const selected = visibleRows
    .map(({ row }) => row)
    .filter((row) => selectedRows.has(row.id) && row.editable !== false);
  const sameType = selected.every((row) => row.type === selected[0]?.type);

  return (
    <div className="editor-table-layout" data-layout={layout}>
      {!unified && (
        <nav className="editor-sections" aria-label="Bank sections">
          <h2>Sections</h2>
          {sections.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-current={item.id === section?.id ? "true" : undefined}
              onClick={() => {
                setSectionId(item.id);
                setSelectedRows(new Set());
                setBulkValue("");
              }}
            >
              {item.name || "Unnamed section"}
            </button>
          ))}
        </nav>
      )}
      <div className="editor-table-scroll">
        {!section ? (
          <p className="empty-message">No sections found.</p>
        ) : (
          <>
            {canEdit && selected.length > 1 && (
              <div className="editor-bulk">
                <label>
                  Set value for {selected.length} rows
                  <input
                    value={bulkValue}
                    onChange={(event) =>
                      setBulkValue(event.currentTarget.value)
                    }
                    disabled={!sameType}
                  />
                </label>
                <button
                  className="editor-button"
                  type="button"
                  disabled={!sameType}
                  onClick={() => onChange(selected, bulkValue)}
                >
                  Stage values
                </button>
                {!sameType && (
                  <p className="empty-message">Select rows of the same type.</p>
                )}
              </div>
            )}
            {visibleRows.length === 0 ? (
              <p className="empty-message">
                {unified ? "No values found." : "No values in this section."}
              </p>
            ) : (
              <table
                aria-label={
                  unified
                    ? "All bank values"
                    : section.name || "Unnamed section"
                }
              >
                <thead>
                  <tr>
                    {unified && <th scope="col">Section</th>}
                    <th scope="col">Name</th>
                    <th scope="col">Type</th>
                    <th scope="col">Value</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleRows.map(({ row, sectionName }) => {
                    const editable = canEdit && row.editable !== false;
                    const edited = values?.get(row.id);
                    const value = edited ?? row.value;
                    const shownValue =
                      groupNumbers && edited === undefined && row.id !== editing
                        ? groupNumberLexeme(row.type, value)
                        : value;
                    const invalidValue = invalid?.has(row.id) ?? false;
                    const name = rowLabel(row);
                    return (
                      <tr key={row.id}>
                        {unified && <td>{sectionName || "Unnamed section"}</td>}
                        <th scope="row">
                          {editable && (
                            <input
                              type="checkbox"
                              aria-label={`Select ${name}`}
                              checked={selectedRows.has(row.id)}
                              onChange={(event) => {
                                const checked = event.currentTarget.checked;
                                setSelectedRows((current) => {
                                  const next = new Set(current);
                                  if (checked) next.add(row.id);
                                  else next.delete(row.id);
                                  return next;
                                });
                              }}
                            />
                          )}
                          {name || "Unnamed key"}
                        </th>
                        <td>{row.type || "Unknown"}</td>
                        <td>
                          {editable || values?.has(row.id) ? (
                            <input
                              type="text"
                              inputMode={
                                row.type === "fixed" ? "decimal" : "text"
                              }
                              aria-label={`${name} value`}
                              aria-invalid={invalidValue || undefined}
                              aria-describedby={
                                invalidValue ? instructionsId : undefined
                              }
                              readOnly={!editable}
                              value={shownValue}
                              onFocus={() => setEditing(row.id)}
                              onBlur={() => setEditing(undefined)}
                              onChange={(event) =>
                                onChange([row], event.currentTarget.value)
                              }
                            />
                          ) : (
                            <span>{shownValue}</span>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            )}
          </>
        )}
        {invalid && invalid.size > 0 && (
          <p id={instructionsId} className="editor-error" role="status">
            Check the marked values. Use commas only between three-digit groups.
          </p>
        )}
      </div>
    </div>
  );
}
