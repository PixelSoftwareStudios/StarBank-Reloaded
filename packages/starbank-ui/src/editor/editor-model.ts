import type {
  EngineDiff,
  EngineRevertTarget,
  EngineStructureChange,
  EngineValueValidationIssue,
} from "../engine/protocol";
import type { EditorProfileController } from "./simple-profile-model";

export interface EditorTarget {
  id: string;
  revision: string;
}

export interface EditorRow {
  id: string;
  name: string;
  childName?: string;
  keyId?: string;
  type: string;
  value: string;
  editable?: boolean;
}

export interface EditorKey {
  id: string;
  name: string;
}

export interface EditorSection {
  id: string;
  name: string;
  dirty?: boolean;
  keys?: readonly EditorKey[];
  rows: readonly EditorRow[];
}

export interface BankEditorDocument extends EditorTarget {
  name: string;
  breadcrumbs?: readonly string[];
  xml: string;
  dirty: boolean;
  dirtySectionCount?: number;
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

export type EditorDiffResult =
  | { ok: true; diff: EngineDiff; message?: string }
  | { ok: false; message: string };

export type EditorAction = (
  target: EditorTarget,
) => Promise<EditorActionResult>;

export interface BankEditorController {
  profile?: EditorProfileController;
  document?: BankEditorDocument;
  busy?: boolean;
  status?: string;
  backupStatus?: string;
  onOpen?: () => Promise<EditorActionResult>;
  onReveal?: EditorAction;
  onOpenFile?: (file: File) => Promise<EditorActionResult>;
  onValidateValues?: (
    target: EditorTarget,
    edits: readonly EditorValueEdit[],
  ) => Promise<readonly EngineValueValidationIssue[]>;
  onEditValues?: (
    target: EditorTarget,
    edits: readonly EditorValueEdit[],
  ) => Promise<EditorActionResult>;
  onReplaceXml?: (
    target: EditorTarget,
    xml: string,
  ) => Promise<EditorActionResult>;
  onDraftChange?: (hasDraft: boolean) => void;
  onStructure?: (
    target: EditorTarget,
    change: EngineStructureChange,
  ) => Promise<EditorActionResult>;
  onDiff?: (target: EditorTarget) => Promise<EditorDiffResult>;
  onRevert?: (
    target: EditorTarget,
    selection: EngineRevertTarget,
  ) => Promise<EditorActionResult>;
  onSave?: EditorAction;
  onSaveAs?: EditorAction;
  onSaveWithBackup?: EditorAction;
  onBackupHistory?: EditorAction;
  canRestoreBackup?: boolean;
  onResign?: EditorAction;
  onExport?: EditorAction;
  onCopy?: EditorAction;
  onReset?: EditorAction;
  onRevertToSaved?: EditorAction;
  onUndo?: EditorAction;
  onRedo?: EditorAction;
}

export type EditorDraft =
  | { kind: "table"; revision: string; values: ReadonlyMap<string, string> }
  | { kind: "raw"; revision: string; text: string };
