export interface EditorTarget {
  id: string;
  revision: string;
}

export interface EditorRow {
  id: string;
  name: string;
  childName?: string;
  type: string;
  value: string;
  editable?: boolean;
}

export interface EditorSection {
  id: string;
  name: string;
  rows: readonly EditorRow[];
}

export interface BankEditorDocument extends EditorTarget {
  name: string;
  xml: string;
  dirty: boolean;
  sections: readonly EditorSection[];
  signatureStatus?: string;
}

export interface EditorValueEdit {
  id: string;
  value: string;
}

export type EditorActionResult =
  | { ok: true; message?: string }
  | { ok: false; message: string };

export type EditorAction = (
  target: EditorTarget,
) => Promise<EditorActionResult>;

export interface BankEditorController {
  document?: BankEditorDocument;
  busy?: boolean;
  status?: string;
  onOpen?: () => Promise<EditorActionResult>;
  onOpenFile?: (file: File) => Promise<EditorActionResult>;
  onEditValues?: (
    target: EditorTarget,
    edits: readonly EditorValueEdit[],
  ) => Promise<EditorActionResult>;
  onReplaceXml?: (
    target: EditorTarget,
    xml: string,
  ) => Promise<EditorActionResult>;
  onSave?: EditorAction;
  onSaveAs?: EditorAction;
  onExport?: EditorAction;
  onCopy?: EditorAction;
  onReset?: EditorAction;
  onUndo?: EditorAction;
  onRedo?: EditorAction;
}

export type EditorDraft =
  | { kind: "table"; revision: string; values: ReadonlyMap<string, string> }
  | { kind: "raw"; revision: string; text: string };

const GROUPED_INTEGER = /^[+-]?\d{1,3}(?:,\d{3})+$/;
const GROUPED_FIXED = /^[+-]?\d{1,3}(?:,\d{3})+(?:\.\d+)?$/;

export function normalizeDraft(row: EditorRow, value: string): string | null {
  if (!value.includes(",")) return value;

  const grouping =
    row.type === "int"
      ? GROUPED_INTEGER
      : row.type === "fixed"
        ? GROUPED_FIXED
        : null;
  if (!grouping) return value;
  return grouping.test(value) ? value.replaceAll(",", "") : null;
}

export function prepareTableDraft(
  sections: readonly EditorSection[],
  values: ReadonlyMap<string, string>,
): { edits: EditorValueEdit[]; invalid: ReadonlySet<string> } {
  const edits: EditorValueEdit[] = [];
  const invalid = new Set(values.keys());

  for (const section of sections) {
    for (const row of section.rows) {
      const draft = values.get(row.id);
      if (draft === undefined || row.editable === false) continue;
      const value = normalizeDraft(row, draft);
      if (value === null) continue;
      invalid.delete(row.id);
      if (value !== row.value) edits.push({ id: row.id, value });
    }
  }
  return { edits, invalid };
}
