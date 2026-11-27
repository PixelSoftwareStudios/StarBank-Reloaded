import { invoke } from "@tauri-apps/api/core";
import type {
  EngineSignatureStatus,
  EngineSigningContext,
} from "starbank-engine-client/protocol";
import * as v from "valibot";

const HEADER_BYTES = 5;
const CONTROL_ONLY = 0;
const WITH_ATTACHMENT = 1;
const CONTROL_LENGTH_OFFSET = 1;
const UNPAIRED_SURROGATE = /[\uD800-\uDFFF]/u;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

export const fileDialogReply = v.object({
  name: v.string(),
  control: v.string(),
  can_overwrite: v.optional(v.boolean()),
  breadcrumbs: v.optional(v.array(v.string())),
});

const optionalFileReply = v.nullable(fileDialogReply);
const count = v.pipe(v.number(), v.safeInteger(), v.minValue(0));
const backupChoices = v.object({
  records: v.array(
    v.object({
      id: count,
      name: v.string(),
      timestamp: v.string(),
      placement: v.picklist(["adjacent", "managed"]),
      health: v.picklist(["verified", "missing", "unverified"]),
    }),
  ),
  rejectedMetadata: count,
  unavailableLocations: count,
});

export type FileDialogReply = v.InferOutput<typeof fileDialogReply>;
export type BackupChoices = v.InferOutput<typeof backupChoices>;

interface NativeReply {
  control: string;
  bytes?: Uint8Array;
}

export function pickBank(control: string): Promise<FileDialogReply | null> {
  return invokeChecked(optionalFileReply, "open_bank", { control });
}

export async function invokeChecked<Schema extends v.GenericSchema>(
  schema: Schema,
  command: string,
  args: Record<string, unknown>,
): Promise<v.InferOutput<Schema>> {
  const result = v.safeParse(schema, await invoke<unknown>(command, args));
  if (!result.success) {
    throw new Error("StarBank received an invalid desktop response.");
  }
  return result.output;
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
  return invokeChecked(optionalFileReply, "export_bank_copy", {
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

export function confirmBankNavigation(
  target: ActiveBankTarget,
  canSave: boolean,
) {
  return invokeChecked(
    v.picklist(["save", "discard", "cancel"]),
    "confirm_bank_navigation",
    { target, canSave },
  );
}

export function saveActiveBank(
  target: ActiveBankTarget,
  withBackup: boolean,
): Promise<FileDialogReply | null> {
  return invokeChecked(optionalFileReply, "save_active_bank", {
    target,
    withBackup,
  });
}

export function saveBankAs(
  target: ActiveBankTarget,
): Promise<FileDialogReply | null> {
  return invokeChecked(optionalFileReply, "save_bank_as", { target });
}

export function listActiveBackups(
  target: ActiveBankTarget,
): Promise<BackupChoices> {
  return invokeChecked(backupChoices, "list_active_backups", { target });
}

export function exportActiveBackup(
  target: ActiveBankTarget,
  backupId: number,
): Promise<string | null> {
  return invokeChecked(v.nullable(v.string()), "export_active_backup", {
    target,
    backupId,
  });
}

const lineNumber = v.pipe(v.number(), v.safeInteger(), v.minValue(1));
const xmlExcerpt = v.object({
  text: v.string(),
  omitsStart: v.boolean(),
  omitsEnd: v.boolean(),
});
const comparisonLines = {
  firstDifferenceLine: v.nullable(lineNumber),
  startLine: lineNumber,
};
const xmlComparison = v.object({
  ...comparisonLines,
  before: xmlExcerpt,
  after: xmlExcerpt,
});
const backupRestoreReview = v.object({
  description: v.string(),
  comparison: xmlComparison,
});
const conflictReview = v.object({
  id: v.pipe(v.string(), v.nonEmpty()),
  ...comparisonLines,
  editor: xmlExcerpt,
  onDisk: xmlExcerpt,
});
const backupPresence = v.picklist(["never_backed_up", "recorded", "unknown"]);
const activeBackupStatus = v.object({
  state: backupPresence,
  recordCount: count,
  rejectedMetadata: count,
  latestTimestampUtc: v.nullable(v.string()),
  incomplete: v.boolean(),
});
const inventoryBackupStatuses = v.array(
  v.object({
    bankId: v.string(),
    state: backupPresence,
  }),
);

export type BackupRestoreReview = v.InferOutput<typeof backupRestoreReview>;
export type XmlComparison = v.InferOutput<typeof xmlComparison>;
export type ConflictReview = v.InferOutput<typeof conflictReview>;
export type ActiveBackupStatus = v.InferOutput<typeof activeBackupStatus>;

export function readActiveBackupStatus(target: ActiveBankTarget) {
  return invokeChecked(activeBackupStatus, "active_backup_status", { target });
}

export function readInventoryBackupStatuses(generation: string) {
  return invokeChecked(inventoryBackupStatuses, "inventory_backup_status", {
    generation,
  });
}

export function reviewActiveBackup(
  target: ActiveBankTarget,
  backupId: number,
): Promise<BackupRestoreReview> {
  return invokeChecked(backupRestoreReview, "review_active_backup", {
    target,
    backupId,
  });
}

export function restoreActiveBank(
  target: ActiveBankTarget,
  backupId: number,
): Promise<FileDialogReply | null> {
  return invokeChecked(optionalFileReply, "restore_active_bank", {
    target,
    backupId,
  });
}

export function compareBankConflict(target: ActiveBankTarget) {
  return invokeChecked(conflictReview, "compare_bank_conflict", { target });
}

export function reopenBankConflict(target: ActiveBankTarget, reviewId: string) {
  return invokeChecked(optionalFileReply, "reopen_bank_conflict", {
    target,
    reviewId,
  });
}
