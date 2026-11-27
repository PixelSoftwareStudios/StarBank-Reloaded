import {
  type FocusEvent,
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
  useCallback,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { EditorRow } from "starbank-editor";

const OVERSCAN_ROWS = 8;
const INITIAL_PAGE_ROWS = 20;
const INITIAL_ROW_HEIGHT = 34;

export interface TableRowProjection {
  row: EditorRow;
  sectionName: string;
  sectionDirty?: boolean;
}

interface WindowPosition {
  first: number;
  count: number;
  rowHeight: number;
}

type Field = "select" | "value";

type RowRenderer = (entry: TableRowProjection, index: number) => ReactElement;

interface WindowedTableProps {
  rows: readonly TableRowProjection[];
  viewKey: string;
  label: string;
  columns: number;
  header: ReactNode;
  canEdit: boolean;
  renderRow: RowRenderer;
}

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, offset) => start + offset);
}

export function WindowedTable({
  rows,
  viewKey,
  label,
  columns,
  header,
  canEdit,
  renderRow,
}: WindowedTableProps) {
  const tableId = useId();
  const viewport = useRef<HTMLDivElement>(null);
  const heading = useRef<HTMLTableSectionElement>(null);
  const [focusId, setFocusId] = useState<string>();
  const [keyFocus, setKeyFocus] = useState<{ index: number; field: Field }>();
  const [position, setPosition] = useState<WindowPosition>({
    first: 0,
    count: INITIAL_PAGE_ROWS,
    rowHeight: INITIAL_ROW_HEIGHT,
  });
  const byId = useMemo(
    () => new Map(rows.map(({ row }, index) => [row.id, index])),
    [rows],
  );

  const refresh = useCallback(() => {
    const scroll = viewport.current;
    if (!scroll || scroll.getClientRects().length === 0) return;
    const measured = scroll
      .querySelector("[data-bank-row-index]")
      ?.getBoundingClientRect().height;
    const available =
      Math.min(scroll.clientHeight, window.innerHeight) -
      (heading.current?.offsetHeight ?? 0);
    setPosition((previous) => {
      const rowHeight =
        measured && measured > 0 ? measured : previous.rowHeight;
      const first = Math.max(0, Math.floor(scroll.scrollTop / rowHeight));
      const count = Math.max(1, Math.ceil(available / rowHeight));
      return first === previous.first &&
        count === previous.count &&
        rowHeight === previous.rowHeight
        ? previous
        : { first, count, rowHeight };
    });
  }, []);

  useLayoutEffect(() => {
    const scroll = viewport.current;
    const head = heading.current;
    if (!scroll || !head) return;
    refresh();
    const observer = new ResizeObserver(refresh);
    observer.observe(scroll);
    observer.observe(head);
    return () => observer.disconnect();
  }, [refresh]);

  useLayoutEffect(() => {
    if (viewport.current) viewport.current.scrollTop = 0;
    refresh();
  }, [refresh, viewKey]);

  const first = Math.min(position.first, Math.max(0, rows.length - 1));
  const start = Math.max(0, first - OVERSCAN_ROWS);
  const end = Math.min(rows.length, first + position.count + OVERSCAN_ROWS);
  const pinned = focusId === undefined ? undefined : byId.get(focusId);
  const indices = range(start, end);
  if (pinned !== undefined && pinned < start) indices.unshift(pinned);
  if (pinned !== undefined && pinned >= end) indices.push(pinned);

  function elementAt(index: number, field: Field) {
    return viewport.current?.querySelector<HTMLInputElement>(
      `[data-bank-row-index="${index}"] [data-bank-field="${field}"]`,
    );
  }

  function scrollTo(index: number) {
    const scroll = viewport.current;
    if (!scroll) return;
    scroll.scrollTop = Math.max(0, index) * position.rowHeight;
    refresh();
  }

  useLayoutEffect(() => {
    if (!keyFocus) return;
    viewport.current
      ?.querySelector<HTMLInputElement>(
        `[data-bank-row-index="${keyFocus.index}"] [data-bank-field="${keyFocus.field}"]`,
      )
      ?.focus({ preventScroll: true });
    setKeyFocus(undefined);
  }, [keyFocus]);

  function trackFocus(event: FocusEvent<HTMLTableElement>) {
    const node = event.target.closest<HTMLTableRowElement>(
      "[data-bank-row-index]",
    );
    const index = Number(node?.dataset.bankRowIndex);
    setFocusId(rows[index]?.row.id);
  }

  function releaseFocus(event: FocusEvent<HTMLTableElement>) {
    if (!event.currentTarget.contains(event.relatedTarget))
      setFocusId(undefined);
  }

  function moveTab(event: KeyboardEvent<HTMLTableElement>) {
    if (event.key !== "Tab" || !(event.target instanceof HTMLInputElement))
      return;
    const row = event.target.closest<HTMLTableRowElement>(
      "[data-bank-row-index]",
    );
    const index = Number(row?.dataset.bankRowIndex);
    const field = event.target.dataset.bankField;
    if (!field || !Number.isInteger(index)) return;
    const editable = canEdit && rows[index]?.row.editable !== false;
    if (
      (!event.shiftKey && field === "select") ||
      (event.shiftKey && field === "value" && editable)
    ) {
      return;
    }
    const next = index + (event.shiftKey ? -1 : 1);
    const entry = rows[next];
    if (!entry) return;
    const nextField =
      event.shiftKey || !canEdit || entry.row.editable === false
        ? "value"
        : "select";
    if (elementAt(next, nextField)) return;
    event.preventDefault();
    scrollTo(next);
    setKeyFocus({ index: next, field: nextField });
  }

  function spacer(from: number, to: number) {
    return (
      // biome-ignore lint/a11y/noAriaHiddenOnFocusable: The inert spacer contains only an empty cell.
      <tr
        aria-hidden="true"
        inert
        className="editor-table-spacer"
        key={`gap-${from}`}
      >
        <td
          colSpan={columns}
          style={{ height: (to - from) * position.rowHeight }}
        />
      </tr>
    );
  }

  const body: ReactElement[] = [];
  let next = 0;
  for (const index of indices) {
    if (index > next) body.push(spacer(next, index));
    const entry = rows[index];
    if (entry) body.push(renderRow(entry, index));
    next = index + 1;
  }
  if (next < rows.length) body.push(spacer(next, rows.length));
  const last = Math.min(rows.length, first + position.count);

  return (
    <>
      <nav className="editor-table-navigation" aria-label="Bank row navigation">
        <button
          aria-controls={tableId}
          className="editor-button"
          disabled={first === 0}
          type="button"
          onClick={() => scrollTo(first - position.count)}
        >
          Previous rows
        </button>
        <span aria-live="polite">
          Rows {first + 1}–{last} of {rows.length}
        </span>
        <button
          aria-controls={tableId}
          className="editor-button"
          disabled={last >= rows.length}
          type="button"
          onClick={() => scrollTo(first + position.count)}
        >
          Next rows
        </button>
      </nav>
      <div className="editor-windowed-table" ref={viewport} onScroll={refresh}>
        <table
          aria-label={label}
          aria-rowcount={rows.length + 1}
          id={tableId}
          onFocus={trackFocus}
          onBlur={releaseFocus}
          onKeyDown={moveTab}
        >
          <thead ref={heading}>
            <tr aria-rowindex={1}>{header}</tr>
          </thead>
          <tbody>{body}</tbody>
        </table>
      </div>
    </>
  );
}
