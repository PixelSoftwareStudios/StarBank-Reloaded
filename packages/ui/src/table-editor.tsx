import { useId, useState } from "react";
import type { EditorRow, EditorSection } from "./editor-model";

function rowLabel(row: EditorRow) {
  return row.childName ? `${row.name} · ${row.childName}` : row.name;
}

export function TableEditor({
  sections,
  values,
  invalid,
  canEdit,
  onChange,
}: {
  sections: readonly EditorSection[];
  values?: ReadonlyMap<string, string>;
  invalid?: ReadonlySet<string>;
  canEdit: boolean;
  onChange: (rows: readonly EditorRow[], value: string) => void;
}) {
  const instructionsId = useId();
  const [sectionId, setSectionId] = useState<string>();
  const [selectedRows, setSelectedRows] = useState<ReadonlySet<string>>(
    new Set(),
  );
  const [bulkValue, setBulkValue] = useState("");
  const section = sections.find(({ id }) => id === sectionId) ?? sections[0];
  const selected =
    section?.rows.filter(
      (row) => selectedRows.has(row.id) && row.editable !== false,
    ) ?? [];
  const sameType = selected.every((row) => row.type === selected[0]?.type);

  return (
    <div className="editor-table-layout">
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
            {section.rows.length === 0 ? (
              <p className="empty-message">No values in this section.</p>
            ) : (
              <table aria-label={section.name || "Unnamed section"}>
                <thead>
                  <tr>
                    <th scope="col">Name</th>
                    <th scope="col">Type</th>
                    <th scope="col">Value</th>
                  </tr>
                </thead>
                <tbody>
                  {section.rows.map((row) => {
                    const editable = canEdit && row.editable !== false;
                    const value = values?.get(row.id) ?? row.value;
                    const invalidValue = invalid?.has(row.id) ?? false;
                    const name = rowLabel(row);
                    return (
                      <tr key={row.id}>
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
                              value={value}
                              onChange={(event) =>
                                onChange([row], event.currentTarget.value)
                              }
                            />
                          ) : (
                            <span>{value}</span>
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
