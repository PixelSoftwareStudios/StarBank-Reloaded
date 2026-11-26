import { ChevronDown, ChevronUp } from "lucide-react";
import { useId, useMemo, useState } from "react";
import {
  type EditorRow,
  type EditorSection,
  groupNumberLexeme,
} from "starbank-editor";
import { type TableRowProjection, WindowedTable } from "./windowed-table";

const WINDOW_THRESHOLD = 150;
const nameOrder = new Intl.Collator(undefined, { sensitivity: "base" });

type SortOrder = "source" | "ascending" | "descending";

const nextSort: Record<SortOrder, SortOrder> = {
  source: "ascending",
  ascending: "descending",
  descending: "source",
};

const sortAction: Record<SortOrder, string> = {
  source: "Sort by name ascending",
  ascending: "Sort by name descending",
  descending: "Restore source order",
};

function filterText(value: string) {
  return value.trim().toLowerCase();
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
  actionsDisabled,
  onOpenStructure,
  onOpenDiff,
}: {
  sections: readonly EditorSection[];
  values?: ReadonlyMap<string, string>;
  invalid?: ReadonlySet<string>;
  canEdit: boolean;
  onChange: (rows: readonly EditorRow[], value: string) => void;
  layout?: "separate" | "unified";
  groupNumbers?: boolean;
  actionsDisabled?: boolean;
  onOpenStructure?: (sectionId?: string) => void;
  onOpenDiff?: () => void;
}) {
  const instructionsId = useId();
  const [sectionId, setSectionId] = useState<string>();
  const [sectionQuery, setSectionQuery] = useState("");
  const [rowQuery, setRowQuery] = useState("");
  const [sort, setSort] = useState<SortOrder>("source");
  const [selectedRows, setSelectedRows] = useState<ReadonlySet<string>>(
    new Set(),
  );
  const [bulkValue, setBulkValue] = useState("");
  const [editing, setEditing] = useState<string>();
  const section = sections.find(({ id }) => id === sectionId) ?? sections[0];
  const unified = layout === "unified";
  const query = filterText(rowQuery);
  const sectionFilter = sections.length > 1 ? filterText(sectionQuery) : "";
  const filteredSections = useMemo(
    () =>
      sections.filter((item) =>
        item.name.toLowerCase().includes(sectionFilter),
      ),
    [sectionFilter, sections],
  );
  const rowsInScope = useMemo(() => {
    const visible = unified ? sections : section ? [section] : [];
    return visible.flatMap((item) =>
      item.rows.map((row) => ({ row, sectionName: item.name })),
    );
  }, [section, sections, unified]);
  const visibleRows = useMemo(() => {
    const matches = query
      ? rowsInScope.filter(
          ({ row, sectionName }) =>
            rowLabel(row).toLowerCase().includes(query) ||
            (unified && sectionName.toLowerCase().includes(query)),
        )
      : rowsInScope;
    if (sort === "source") return matches;
    return [...matches].sort(({ row: a }, { row: b }) =>
      sort === "ascending"
        ? nameOrder.compare(a.name, b.name)
        : nameOrder.compare(b.name, a.name),
    );
  }, [query, rowsInScope, sort, unified]);
  const rowsById = useMemo(
    () => new Map(visibleRows.map(({ row }) => [row.id, row])),
    [visibleRows],
  );
  const selected = Array.from(selectedRows, (id) => rowsById.get(id)).filter(
    (row): row is EditorRow => row !== undefined && row.editable !== false,
  );
  const sameType = selected.every((row) => row.type === selected[0]?.type);
  const hasHiddenDraft =
    query && values?.size
      ? rowsInScope.some(
          ({ row }) => values.has(row.id) && !rowsById.has(row.id),
        )
      : false;
  const windowed = visibleRows.length > WINDOW_THRESHOLD;
  const label = unified
    ? "All bank values"
    : section?.name || "Unnamed section";
  const header = (
    <>
      {unified && <th scope="col">Section</th>}
      <th scope="col" aria-sort={sort === "source" ? undefined : sort}>
        <button
          className="editor-name-sort"
          type="button"
          title={sortAction[sort]}
          onClick={() => setSort(nextSort[sort])}
        >
          Name
          {sort === "ascending" && <ChevronUp aria-hidden="true" />}
          {sort === "descending" && <ChevronDown aria-hidden="true" />}
        </button>
      </th>
      <th scope="col">Type</th>
      <th scope="col">Value</th>
    </>
  );

  function renderRow({ row, sectionName }: TableRowProjection, index?: number) {
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
      <tr
        key={row.id}
        aria-rowindex={index === undefined ? undefined : index + 2}
        data-bank-row-index={index}
      >
        {unified && (
          <td title={windowed ? sectionName : undefined}>
            {sectionName || "Unnamed section"}
          </td>
        )}
        <th scope="row" title={windowed ? name : undefined}>
          {editable && (
            <input
              type="checkbox"
              aria-label={`Select ${name}`}
              data-bank-field={windowed ? "select" : undefined}
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
        <td title={windowed ? row.type : undefined}>{row.type || "Unknown"}</td>
        <td>
          {windowed || editable || values?.has(row.id) ? (
            <input
              type="text"
              inputMode={row.type === "fixed" ? "decimal" : "text"}
              aria-label={`${name} value`}
              aria-invalid={invalidValue || undefined}
              aria-describedby={invalidValue ? instructionsId : undefined}
              data-bank-field={windowed ? "value" : undefined}
              readOnly={!editable}
              value={shownValue}
              onFocus={() => setEditing(row.id)}
              onBlur={() => setEditing(undefined)}
              onChange={(event) => onChange([row], event.currentTarget.value)}
            />
          ) : (
            <span>{shownValue}</span>
          )}
        </td>
      </tr>
    );
  }

  return (
    <div
      className="editor-table-layout"
      data-layout={layout}
      data-windowed={windowed || undefined}
    >
      {!unified && (
        <nav className="editor-sections" aria-label="Bank sections">
          <h2>Sections</h2>
          {sections.length > 1 && (
            <input
              aria-label="Filter sections by name"
              className="editor-section-filter"
              type="search"
              placeholder="Filter…"
              value={sectionQuery}
              onChange={(event) => setSectionQuery(event.currentTarget.value)}
            />
          )}
          {filteredSections.length === 0 && sectionFilter && (
            <p className="empty-message" role="status">
              No matching sections.
            </p>
          )}
          {filteredSections.map((item) => (
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
        {(onOpenStructure || onOpenDiff || section) && (
          <div className="editor-table-actions">
            {section && (
              <input
                aria-label="Filter rows by name"
                data-table-filter-search
                type="search"
                placeholder="Filter names…"
                value={rowQuery}
                onChange={(event) => setRowQuery(event.currentTarget.value)}
              />
            )}
            {onOpenStructure && (
              <button
                className="editor-button"
                type="button"
                disabled={actionsDisabled}
                onClick={() => onOpenStructure(section?.id)}
              >
                Edit structure…
              </button>
            )}
            {onOpenDiff && (
              <button
                className="editor-button"
                type="button"
                disabled={actionsDisabled}
                onClick={onOpenDiff}
              >
                Changes…
              </button>
            )}
          </div>
        )}
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
              <p className="empty-message" role={query ? "status" : undefined}>
                {query
                  ? "No matching rows."
                  : unified
                    ? "No values found."
                    : "No values in this section."}
              </p>
            ) : windowed ? (
              <WindowedTable
                key={unified ? "all-sections" : section.id}
                rows={visibleRows}
                viewKey={`${sort}:${query}`}
                header={header}
                columns={unified ? 4 : 3}
                label={label}
                canEdit={canEdit}
                renderRow={renderRow}
              />
            ) : (
              <table aria-label={label}>
                <thead>
                  <tr>{header}</tr>
                </thead>
                <tbody>{visibleRows.map((entry) => renderRow(entry))}</tbody>
              </table>
            )}
          </>
        )}
        {hasHiddenDraft && (
          <p className="empty-message" role="status">
            The row filter hides some staged edits.
          </p>
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
