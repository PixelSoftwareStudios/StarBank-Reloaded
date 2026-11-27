import { ChevronDown, ChevronUp } from "lucide-react";
import {
  type Ref,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { buttonClasses, focusOutlineClasses } from "../shared/control-styles";
import { type EditorRow, type EditorSection, groupNumberLexeme } from "./index";
import { useNumericEditingFocus } from "./numeric-editing-focus";
import { type TableRowProjection, WindowedTable } from "./windowed-table";

const WINDOW_THRESHOLD = 150;
const tableFieldClasses = `min-w-0 rounded-control border border-control-edge bg-field px-[0.45rem] py-[0.2rem] tabular-nums aria-invalid:border-[color:var(--editor-danger)] ${focusOutlineClasses}`;
const headerCellClasses =
  "border-b border-line px-1.5 py-[0.235rem] text-start wrap-anywhere first:w-[32%] [&:nth-child(2)]:w-[17%]";
const tableErrorClasses =
  "editor-error p-[0.3rem] text-[color:var(--editor-danger)]";
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

export interface TableEditorHandle {
  repairIssue: (rowId: string) => boolean;
}

export function TableEditor({
  ref,
  sections,
  values,
  issues,
  validationError,
  canEdit,
  onChange,
  layout = "separate",
  groupNumbers = false,
  actionsDisabled,
  onOpenStructure,
  onOpenDiff,
}: {
  ref?: Ref<TableEditorHandle>;
  sections: readonly EditorSection[];
  values?: ReadonlyMap<string, string>;
  issues?: ReadonlyMap<string, string>;
  validationError?: string;
  canEdit: boolean;
  onChange: (rows: readonly EditorRow[], value: string) => void;
  layout?: "separate" | "unified";
  groupNumbers?: boolean;
  actionsDisabled?: boolean;
  onOpenStructure?: (sectionId?: string) => void;
  onOpenDiff?: () => void;
}) {
  const issueDescriptionId = useId();
  const [sectionId, setSectionId] = useState<string>();
  const [sectionQuery, setSectionQuery] = useState("");
  const [rowQuery, setRowQuery] = useState("");
  const [sort, setSort] = useState<SortOrder>("source");
  const [selectedRows, setSelectedRows] = useState<ReadonlySet<string>>(
    new Set(),
  );
  const selectionAnchor = useRef<string | undefined>(undefined);
  const [bulkValue, setBulkValue] = useState("");
  const editing = useNumericEditingFocus();
  const [revealed, setRevealed] = useState<{
    rowId: string;
    sectionId: string;
  }>();
  const [repairFocusId, setRepairFocusId] = useState<string>();
  const repairField = useRef<HTMLInputElement>(null);
  const section = sections.find(({ id }) => id === sectionId) ?? sections[0];
  const unified = layout === "unified";
  const query = filterText(rowQuery);
  const sectionFilter = sections.length > 1 ? filterText(sectionQuery) : "";
  const filteredSections = useMemo(
    () =>
      sections.filter(
        (item) =>
          item.id === revealed?.sectionId ||
          item.name.toLowerCase().includes(sectionFilter),
      ),
    [revealed, sectionFilter, sections],
  );
  const rowsInScope = useMemo(() => {
    const visible = unified ? sections : section ? [section] : [];
    return visible.flatMap((item) =>
      item.rows.map((row) => ({
        row,
        sectionName: item.name,
        sectionDirty: item.dirty,
      })),
    );
  }, [section, sections, unified]);
  const visibleRows = useMemo(() => {
    const matches = query
      ? rowsInScope.filter(
          ({ row, sectionName }) =>
            row.id === revealed?.rowId ||
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
  }, [query, revealed, rowsInScope, sort, unified]);
  const rowsById = useMemo(
    () =>
      new Map(visibleRows.map(({ row }, index) => [row.id, { row, index }])),
    [visibleRows],
  );
  const selected = Array.from(
    selectedRows,
    (id) => rowsById.get(id)?.row,
  ).filter(
    (row): row is EditorRow => row !== undefined && row.editable !== false,
  );
  const sameType = selected.every((row) => row.type === selected[0]?.type);
  const first = selected[0];
  const firstValue = first && (values?.get(first.id) ?? first.value);
  const mixedValues = selected.some(
    (row) => (values?.get(row.id) ?? row.value) !== firstValue,
  );
  const hasHiddenDraft =
    query && values?.size
      ? rowsInScope.some(
          ({ row }) => values.has(row.id) && !rowsById.has(row.id),
        )
      : false;
  const windowed = visibleRows.length > WINDOW_THRESHOLD;
  useImperativeHandle(
    ref,
    () => ({
      repairIssue(rowId) {
        if (!canEdit || !issues?.has(rowId)) return false;
        const location = sections.find(({ rows }) =>
          rows.some((row) => row.id === rowId && row.editable !== false),
        );
        if (!location) return false;
        if (!unified) setSectionId(location.id);
        setRevealed({ rowId, sectionId: location.id });
        setRepairFocusId(rowId);
        return true;
      },
    }),
    [canEdit, issues, sections, unified],
  );

  useLayoutEffect(() => {
    if (!repairFocusId) return;
    const field = repairField.current;
    if (field && !field.readOnly && issues?.has(repairFocusId)) {
      const viewport = field.closest(".editor-windowed-table");
      const header = viewport?.querySelector("thead");
      if (viewport && header) {
        const bounds = field.getBoundingClientRect();
        const viewportBounds = viewport.getBoundingClientRect();
        const top = header.getBoundingClientRect().bottom;
        const bottom = viewportBounds.bottom;
        if (top < bottom && (bounds.top < top || bounds.bottom > bottom)) {
          // Center in the visible rows below the sticky heading.
          viewport.scrollTop += (bounds.top + bounds.bottom - top - bottom) / 2;
        }
        viewport.scrollIntoView({ block: "nearest", inline: "nearest" });
      }
      field.focus({ preventScroll: viewport !== null });
    }
    setRepairFocusId(undefined);
  }, [issues, repairFocusId]);

  const label = unified
    ? "All bank values"
    : section?.name || "Unnamed section";
  const header = (
    <>
      {unified && (
        <th className={headerCellClasses} scope="col">
          Section
        </th>
      )}
      <th
        className={headerCellClasses}
        scope="col"
        aria-sort={sort === "source" ? undefined : sort}
      >
        <button
          className={`editor-name-sort inline-flex items-center gap-[0.2rem] rounded-[0.2rem] text-start ${focusOutlineClasses}`}
          type="button"
          title={sortAction[sort]}
          onClick={() => setSort(nextSort[sort])}
        >
          Name
          {sort === "ascending" && (
            <ChevronUp aria-hidden="true" className="size-3 flex-none" />
          )}
          {sort === "descending" && (
            <ChevronDown aria-hidden="true" className="size-3 flex-none" />
          )}
        </button>
      </th>
      <th className={headerCellClasses} scope="col">
        Type
      </th>
      <th className={headerCellClasses} scope="col">
        Value
      </th>
    </>
  );

  function selectRow(id: string, checked: boolean, extend: boolean) {
    const target = rowsById.get(id);
    if (!target || target.row.editable === false) return;
    const previousId = selectionAnchor.current;
    const anchor =
      extend && previousId !== undefined ? rowsById.get(previousId) : undefined;
    if (!anchor) selectionAnchor.current = id;
    const start = Math.min(anchor?.index ?? target.index, target.index);
    const end = Math.max(anchor?.index ?? target.index, target.index);
    setSelectedRows((current) => {
      const next = new Set(current);
      for (let index = start; index <= end; index += 1) {
        const row = visibleRows[index]?.row;
        if (!row || row.editable === false) continue;
        if (checked) next.add(row.id);
        else next.delete(row.id);
      }
      return next;
    });
  }

  function renderRow(
    { row, sectionName, sectionDirty }: TableRowProjection,
    index?: number,
  ) {
    const editable = canEdit && row.editable !== false;
    const edited = values?.get(row.id);
    const value = edited ?? row.value;
    const groupedValue =
      groupNumbers && edited === undefined
        ? groupNumberLexeme(row.type, value)
        : value;
    const focused = editing.focused?.rowId === row.id;
    const shownValue = focused && editing.focused?.exact ? value : groupedValue;
    const issue = issues?.get(row.id);
    const issueId =
      issue !== undefined ? `${issueDescriptionId}-${row.id}` : undefined;
    const name = rowLabel(row);
    const cellClasses = `border-b border-line px-1.5 text-start wrap-anywhere ${index === undefined ? "py-[0.235rem]" : "h-[var(--editor-windowed-row-height)] truncate py-0"}`;
    return (
      <tr
        key={row.id}
        className="[&:hover]:bg-[var(--subtle-surface)]"
        aria-rowindex={index === undefined ? undefined : index + 2}
        data-bank-row-index={index}
      >
        {unified && (
          <td
            className={cellClasses}
            aria-description={sectionDirty ? "Unsaved changes" : undefined}
            title={windowed ? sectionName : undefined}
          >
            {sectionName || "Unnamed section"}
            {sectionDirty && (
              <span
                className="editor-section-dirty ms-1.5 inline-block size-1.5 rounded-full bg-[var(--editor-unsaved)] align-middle"
                aria-hidden="true"
              />
            )}
          </td>
        )}
        <th
          className={`${cellClasses} font-normal`}
          scope="row"
          title={windowed ? name : undefined}
        >
          {editable && (
            <input
              type="checkbox"
              className={`me-[0.35rem] accent-focus ${focusOutlineClasses}`}
              aria-label={`Select ${name}`}
              data-bank-field={windowed ? "select" : undefined}
              checked={selectedRows.has(row.id)}
              aria-description="Use Shift with a click or Space to select a range."
              onChange={(event) => {
                const gesture = event.nativeEvent;
                selectRow(
                  row.id,
                  event.currentTarget.checked,
                  gesture instanceof MouseEvent && gesture.shiftKey,
                );
              }}
              onKeyDown={(event) => {
                if (event.key !== " " || !event.shiftKey) return;
                event.preventDefault();
                if (!event.repeat) {
                  selectRow(row.id, !event.currentTarget.checked, true);
                }
              }}
            />
          )}
          {name || "Unnamed key"}
        </th>
        <td
          className={`editor-table-type text-[0.8125rem] text-muted ${cellClasses}`}
          title={windowed ? row.type : undefined}
        >
          {row.type || "Unknown"}
        </td>
        <td className={cellClasses}>
          {windowed || editable || focused || values?.has(row.id) ? (
            <input
              type="text"
              className={`${tableFieldClasses} w-full${index === undefined ? "" : " h-[calc(var(--editor-windowed-row-height)_-_0.375rem)]"}`}
              inputMode={row.type === "fixed" ? "decimal" : "text"}
              aria-label={`${name} value`}
              aria-invalid={issue !== undefined || undefined}
              data-table-draft={edited !== undefined || undefined}
              ref={repairFocusId === row.id ? repairField : undefined}
              aria-describedby={issueId}
              data-bank-field={windowed ? "value" : undefined}
              readOnly={!editable}
              value={shownValue}
              {...editing.handlers(row.id, groupedValue, value)}
              onChange={(event) => {
                const field = event.currentTarget;
                editing.inputChanged(row.id, field);
                onChange([row], field.value);
              }}
            />
          ) : (
            <span>{shownValue}</span>
          )}
          {issueId && (
            <span id={issueId} className="sr-only">
              {issue}
            </span>
          )}
        </td>
      </tr>
    );
  }

  return (
    <div
      className={`editor-table-layout grid h-full min-w-0 overflow-hidden ${unified ? "grid-cols-[minmax(0,1fr)]" : "grid-cols-[minmax(6.25rem,22%)_minmax(0,1fr)] max-[31rem]:grid-cols-[minmax(0,1fr)] max-[31rem]:grid-rows-[auto_minmax(0,1fr)]"}`}
      data-layout={layout}
      data-windowed={windowed || undefined}
    >
      {!unified && (
        <nav
          className="editor-sections me-2 min-h-0 min-w-0 overflow-auto border-e border-line pe-2 max-[31rem]:me-0 max-[31rem]:mb-2 max-[31rem]:grid max-[31rem]:grid-cols-2 max-[31rem]:items-center max-[31rem]:gap-x-2 max-[31rem]:border-e-0 max-[31rem]:border-b max-[31rem]:pe-0 max-[31rem]:pb-[0.4rem]"
          aria-label="Bank sections"
        >
          <h2 className="pt-[0.3rem] pb-[0.4rem] font-semibold">Sections</h2>
          {sections.length > 1 && (
            <input
              aria-label="Filter sections by name"
              className={`editor-section-filter mb-1 min-h-[var(--control-height)] w-full rounded-control border border-control-edge bg-field px-[0.45rem] py-[0.2rem] ${focusOutlineClasses}`}
              type="search"
              placeholder="Filter…"
              value={sectionQuery}
              onChange={(event) => {
                setRevealed(undefined);
                setSectionQuery(event.currentTarget.value);
              }}
            />
          )}
          {filteredSections.length === 0 && sectionFilter && (
            <p
              className="empty-message text-muted max-[31rem]:col-span-full"
              role="status"
            >
              No matching sections.
            </p>
          )}
          <div className="editor-section-list flex flex-col gap-0.5 max-[31rem]:col-span-full max-[31rem]:flex-row max-[31rem]:overflow-x-auto max-[31rem]:py-[0.15rem]">
            {filteredSections.map((item) => (
              <button
                key={item.id}
                className={`block w-full rounded-control border border-transparent px-[0.4rem] py-1 text-start wrap-anywhere [&:hover:not([aria-current=true])]:bg-hover aria-[current=true]:border-s-[color:var(--selection-edge)] aria-[current=true]:bg-selected aria-[current=true]:font-semibold max-[31rem]:w-auto max-[31rem]:whitespace-nowrap ${focusOutlineClasses}`}
                type="button"
                aria-current={item.id === section?.id ? "true" : undefined}
                aria-description={item.dirty ? "Unsaved changes" : undefined}
                onClick={() => {
                  setRevealed(undefined);
                  setSectionId(item.id);
                  selectionAnchor.current = undefined;
                  setSelectedRows(new Set());
                  setBulkValue("");
                }}
              >
                {item.name || "Unnamed section"}
                {item.dirty && (
                  <span
                    className="editor-section-dirty ms-1.5 inline-block size-1.5 rounded-full bg-[var(--editor-unsaved)] align-middle"
                    aria-hidden="true"
                  />
                )}
              </button>
            ))}
          </div>
        </nav>
      )}
      <div
        className={`editor-table-scroll min-h-0 min-w-0 ${windowed ? "flex flex-col overflow-hidden" : "overflow-auto"}`}
      >
        {(onOpenStructure || onOpenDiff || section) && (
          <div className="editor-table-actions mb-[0.35rem] flex flex-wrap gap-[0.35rem]">
            {section && (
              <input
                aria-label="Filter rows by name"
                data-table-filter-search
                className={`${tableFieldClasses} w-auto flex-[1_1_8rem]`}
                type="search"
                placeholder="Filter names…"
                value={rowQuery}
                onChange={(event) => {
                  setRevealed(undefined);
                  setRowQuery(event.currentTarget.value);
                }}
              />
            )}
            {onOpenStructure && (
              <button
                className={buttonClasses()}
                type="button"
                disabled={actionsDisabled}
                onClick={() => onOpenStructure(section?.id)}
              >
                Edit structure…
              </button>
            )}
            {onOpenDiff && (
              <button
                className={buttonClasses()}
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
          <p className="empty-message text-muted">No sections found.</p>
        ) : (
          <>
            {canEdit && selected.length > 1 && (
              <div className="editor-bulk flex flex-wrap items-end gap-2 px-[0.3rem] pt-1 pb-2">
                <label className="flex-[1_1_10rem]">
                  Set value for {selected.length} rows
                  <input
                    className={`${tableFieldClasses} block w-full`}
                    placeholder={mixedValues ? "Mixed values" : undefined}
                    value={bulkValue}
                    onChange={(event) =>
                      setBulkValue(event.currentTarget.value)
                    }
                    disabled={!sameType}
                  />
                </label>
                <button
                  className={buttonClasses()}
                  type="button"
                  disabled={!sameType}
                  onClick={() => onChange(selected, bulkValue)}
                >
                  Stage values
                </button>
                {!sameType && (
                  <p className="empty-message text-muted">
                    Select rows of the same type.
                  </p>
                )}
              </div>
            )}
            {visibleRows.length === 0 ? (
              <p
                className="empty-message text-muted"
                role={query ? "status" : undefined}
              >
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
                revealRowId={repairFocusId}
                renderRow={renderRow}
              />
            ) : (
              <table
                className="w-full table-fixed border-collapse"
                aria-label={label}
              >
                <thead className="sticky top-0 z-[1] bg-[var(--subtle-surface)]">
                  <tr>{header}</tr>
                </thead>
                <tbody>{visibleRows.map((entry) => renderRow(entry))}</tbody>
              </table>
            )}
          </>
        )}
        {hasHiddenDraft && (
          <p className="empty-message text-muted" role="status">
            The row filter hides some staged edits.
          </p>
        )}
        {issues && issues.size > 0 && (
          <p className={tableErrorClasses} role="status">
            {[...new Set(issues.values())].join(" ")}
          </p>
        )}
        {validationError && (
          <p className={tableErrorClasses} role="status">
            {validationError}
          </p>
        )}
      </div>
    </div>
  );
}
