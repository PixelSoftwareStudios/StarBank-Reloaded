import { invoke } from "@tauri-apps/api/core";
import type {
  EngineSignatureStatus,
  EngineSigningContext,
} from "starbank-engine-client/protocol";

const HEADER_BYTES = 5;
const CONTROL_ONLY = 0;
const WITH_ATTACHMENT = 1;
const CONTROL_LENGTH_OFFSET = 1;
const UNPAIRED_SURROGATE = /[\uD800-\uDFFF]/u;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

export interface FileDialogReply {
  name: string;
  control: string;
  can_overwrite?: boolean;
  breadcrumbs?: readonly string[];
}

interface NativeReply {
  control: string;
  bytes?: Uint8Array;
}

export function pickBank(control: string): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("open_bank", { control });
}

export type ManualResignAction = {
  readonly kind: "check" | "apply";
  readonly context: EngineSigningContext;
};

type ResignReply =
  | { readonly kind: "manual_context_required" }
  | {
      readonly kind: "checked";
      readonly payload: Exclude<EngineSignatureStatus, "missing_context">;
    }
  | { readonly kind: "signed"; readonly payload: FileDialogReply };

export function resignActiveBank(
  target: ActiveBankTarget,
  manual?: ManualResignAction,
): Promise<ResignReply> {
  return invoke<ResignReply>("resign_active_bank", {
    target,
    manual: manual ?? null,
  });
}

export function canExportBank(): Promise<boolean> {
  return invoke<boolean>("can_export_bank");
}

export function canRevealBank(): Promise<boolean> {
  return invoke<boolean>("can_reveal_bank");
}

export function revealActiveBank(target: ActiveBankTarget): Promise<void> {
  return invoke<void>("reveal_active_bank", { target });
}

export function exportBankCopy(
  control: string,
  suggestedName: string,
): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("export_bank_copy", {
    control,
    suggestedName,
  });
}

export async function dispatchBank(
  control: string,
  bytes?: Uint8Array,
): Promise<NativeReply> {
  const encoded = encoder.encode(control);
  const buffer = new Uint8Array(
    HEADER_BYTES + encoded.length + (bytes?.length ?? 0),
  );
  const frame = new DataView(buffer.buffer);
  frame.setUint8(0, bytes === undefined ? CONTROL_ONLY : WITH_ATTACHMENT);
  frame.setUint32(CONTROL_LENGTH_OFFSET, encoded.length, true);
  buffer.set(encoded, HEADER_BYTES);
  if (bytes !== undefined) {
    buffer.set(bytes, HEADER_BYTES + encoded.length);
  }

  const reply = await invoke<ArrayBuffer>("dispatch_bank", buffer);
  if (!(reply instanceof ArrayBuffer) || reply.byteLength < HEADER_BYTES) {
    throw new Error("The bank engine sent an invalid native reply.");
  }
  const returned = new DataView(reply);
  const flag = returned.getUint8(0);
  const controlEnd =
    HEADER_BYTES + returned.getUint32(CONTROL_LENGTH_OFFSET, true);
  if (
    controlEnd > reply.byteLength ||
    (flag !== WITH_ATTACHMENT &&
      (flag !== CONTROL_ONLY || controlEnd !== reply.byteLength))
  ) {
    throw new Error("The bank engine sent an invalid native reply.");
  }
  return {
    control: decoder.decode(
      new Uint8Array(reply, HEADER_BYTES, controlEnd - HEADER_BYTES),
    ),
    bytes:
      flag === WITH_ATTACHMENT ? new Uint8Array(reply, controlEnd) : undefined,
  };
}

export function decodeBankXml(bytes: Uint8Array): string {
  return decoder.decode(bytes);
}

export function encodeBankXml(xml: string): Uint8Array {
  if (UNPAIRED_SURROGATE.test(xml)) {
    throw new Error("The bank XML contains invalid Unicode text.");
  }
  return encoder.encode(xml);
}

export interface ActiveBankTarget {
  requestId: string;
  documentId: string;
  revision: string;
}

export interface BackupChoice {
  id: number;
  name: string;
  timestamp: string;
  placement: "adjacent" | "managed";
  health: "verified" | "missing" | "unverified";
}

export interface BackupChoices {
  records: readonly BackupChoice[];
  rejectedMetadata: number;
  unavailableLocations: number;
}

export function confirmBankNavigation(
  target: ActiveBankTarget,
  canSave: boolean,
) {
  return invoke<"save" | "discard" | "cancel">("confirm_bank_navigation", {
    target,
    canSave,
  });
}

export function saveActiveBank(
  target: ActiveBankTarget,
  withBackup: boolean,
): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("save_active_bank", {
    target,
    withBackup,
  });
}

export function saveBankAs(
  target: ActiveBankTarget,
): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("save_bank_as", { target });
}

export function listActiveBackups(
  target: ActiveBankTarget,
): Promise<BackupChoices> {
  return invoke<BackupChoices>("list_active_backups", { target });
}

export function exportActiveBackup(
  target: ActiveBankTarget,
  backupId: number,
): Promise<string | null> {
  return invoke<string | null>("export_active_backup", { target, backupId });
}

export function restoreActiveBank(
  target: ActiveBankTarget,
  backupId: number,
): Promise<FileDialogReply | null> {
  return invoke<FileDialogReply | null>("restore_active_bank", {
    target,
    backupId,
  });
}

export interface ConflictExcerpt {
  readonly text: string;
  readonly omitsStart: boolean;
  readonly omitsEnd: boolean;
}

export interface ConflictReview {
  readonly id: string;
  readonly firstDifferenceLine: number | null;
  readonly startLine: number;
  readonly editor: ConflictExcerpt;
  readonly onDisk: ConflictExcerpt;
}

export function compareBankConflict(target: ActiveBankTarget) {
  return invoke<ConflictReview>("compare_bank_conflict", { target });
}

export function reopenBankConflict(target: ActiveBankTarget, reviewId: string) {
  return invoke<FileDialogReply | null>("reopen_bank_conflict", {
    target,
    reviewId,
  });
}
