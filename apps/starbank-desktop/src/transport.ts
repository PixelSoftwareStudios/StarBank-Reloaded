import { Channel, invoke } from "@tauri-apps/api/core";
import {
  type EngineSigningContext,
  signatureStatus,
} from "starbank-ui/engine/signing-protocol";
import { MAX_BACKUP_RETENTION } from "starbank-ui/settings-model";
import * as v from "valibot";
import { recordedPathSchema, recordedReasonSchema } from "#write-recovery";
import type { BatchBackupRequest } from "./batch-backup-ui";

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

const checkedSignature = v.picklist(
  signatureStatus.options.filter((status) => status !== "missing_context"),
);
const resignReply = v.variant("kind", [
  v.object({ kind: v.literal("manual_context_required") }),
  v.object({ kind: v.literal("checked"), payload: checkedSignature }),
  v.object({ kind: v.literal("signed"), payload: fileDialogReply }),
]);
const externalSigningSource = v.nullable(
  v.object({
    id: v.pipe(v.string(), v.nonEmpty()),
    name: v.string(),
    requires_context: v.boolean(),
    signature_status: signatureStatus,
  }),
);

type ResignReply = v.InferOutput<typeof resignReply>;

export function resignActiveBank(
  target: ActiveBankTarget,
  manual?: ManualResignAction,
): Promise<ResignReply> {
  return invokeChecked(resignReply, "resign_active_bank", {
    target,
    manual: manual ?? null,
  });
}

export function pickExternalSigningSource() {
  return invokeChecked(
    externalSigningSource,
    "pick_external_bank_to_resign",
    {},
  );
}

export function checkExternalBankSignature(
  id: string,
  context: EngineSigningContext,
) {
  return invokeChecked(checkedSignature, "check_external_bank_signature", {
    id,
    context,
  });
}

export function exportExternalSignedBank(
  id: string,
  context: EngineSigningContext | null,
) {
  return invokeChecked(v.nullable(v.string()), "export_external_signed_bank", {
    id,
    context,
  });
}

export function canExportBank(): Promise<boolean> {
  return invokeChecked(v.boolean(), "can_export_bank", {});
}

export function canRevealBank(): Promise<boolean> {
  return invokeChecked(v.boolean(), "can_reveal_bank", {});
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

const inventorySnapshot = v.object({
  generation: v.string(),
  status: v.picklist(["ready", "root-not-found", "manual-required", "failed"]),
  reason: v.nullable(
    v.picklist([
      "invalid_folder",
      "unreadable",
      "too_many_entries",
      "too_many_matches",
      "identifier_conflict",
      "configuration_unavailable",
    ]),
  ),
  maps: v.array(
    v.object({
      id: v.string(),
      name: v.string(),
      isBlizzard: v.boolean(),
      hasBankFiles: v.boolean(),
      author: v.nullable(v.string()),
      modifiedAtMillis: v.nullable(v.pipe(v.number(), v.safeInteger())),
    }),
  ),
  banks: v.array(
    v.object({
      id: v.string(),
      name: v.string(),
      scopeId: v.nullable(v.string()),
      playerHandle: v.nullable(v.string()),
      authorHandle: v.nullable(v.string()),
      mapIds: v.array(v.string()),
    }),
  ),
  rootName: v.nullable(v.string()),
  manual: v.boolean(),
  incomplete: v.boolean(),
  canRebuildMapIndex: v.boolean(),
});
const inventoryProgress = v.variant("stage", [
  v.object({
    generation: v.string(),
    stage: v.picklist([
      "locating_folder",
      "enumerating_banks",
      "indexing_maps",
    ]),
  }),
  v.object({
    generation: v.string(),
    stage: v.literal("inspecting_archives"),
    completed: count,
    total: count,
  }),
]);

export type InventorySnapshot = v.InferOutput<typeof inventorySnapshot>;
export type InventoryProgress = v.InferOutput<typeof inventoryProgress>;
export type InventoryQuery =
  | "refresh_inventory"
  | "rebuild_map_index"
  | "use_automatic_inventory"
  | "select_inventory_folder";

export async function readInventory(
  command: InventoryQuery,
  onProgress: (progress: InventoryProgress) => void,
): Promise<InventorySnapshot | null> {
  let receiving = true;
  let generation: string | undefined;
  const channel = new Channel<unknown>((update) => {
    if (!receiving) return;
    const result = v.safeParse(inventoryProgress, update);
    if (!result.success) return;
    const progress = result.output;
    if (generation !== undefined && progress.generation !== generation) return;
    generation = progress.generation;
    onProgress(progress);
  });
  try {
    const result = await invokeChecked(v.nullable(inventorySnapshot), command, {
      progress: channel,
    });
    if (
      result &&
      generation !== undefined &&
      result.generation !== generation
    ) {
      throw new Error("StarBank received an invalid desktop response.");
    }
    return result;
  } finally {
    receiving = false;
  }
}

export const MAX_NATIVE_U32 = 0xffff_ffff;
const nativeRequestId = v.pipe(
  count,
  v.minValue(1),
  v.maxValue(MAX_NATIVE_U32),
);
const openRequests = v.array(
  v.object({ id: nativeRequestId, name: v.string() }),
);
const closeProbe = v.strictObject({ ticket: nativeRequestId });
const closeChoice = v.picklist(["save", "discard", "cancel"]);

export type OpenRequest = v.InferOutput<typeof openRequests>[number];
interface CloseReport {
  ticket: number;
  draftGeneration: number;
  hasDraft: boolean;
  canSave: boolean;
}

export function readOpenRequests() {
  return invokeChecked(openRequests, "list_open_requests", {});
}

export function openRequestedBank(id: number, control: string) {
  return invokeChecked(optionalFileReply, "open_requested_bank", {
    id,
    control,
  });
}

export function decodeCloseProbe(payload: unknown) {
  const result = v.safeParse(closeProbe, payload);
  return result.success ? result.output : undefined;
}

export async function confirmWindowClose(report: CloseReport) {
  try {
    return await invokeChecked(closeChoice, "confirm_window_close", { report });
  } catch (error) {
    await invoke("complete_window_close", {
      ticket: report.ticket,
      draftGeneration: report.draftGeneration,
      proceed: false,
    }).catch(() => {});
    throw error;
  }
}

const MAX_BATCH_BACKUP_ROWS = 4_096;
const MAX_BATCH_RUN_ID = 0xffff_ffff;
const batchRunId = v.pipe(count, v.minValue(1), v.maxValue(MAX_BATCH_RUN_ID));
const batchBackupOutcome = v.picklist([
  "created",
  "source_unavailable",
  "rejected",
  "needs_review",
  "cancelled",
  "inventory_changed",
]);
const batchBackupReport = v.pipe(
  v.strictObject({
    runId: v.nullable(batchRunId),
    entries: v.pipe(
      v.array(
        v.strictObject({
          bankId: v.string(),
          name: v.string(),
          outcome: batchBackupOutcome,
          backupName: v.nullable(v.string()),
        }),
      ),
      v.maxLength(MAX_BATCH_BACKUP_ROWS),
    ),
    created: count,
    failed: count,
    skipped: count,
  }),
  v.check((report) => {
    const totals = { created: 0, failed: 0, skipped: 0 };
    let needsReview = false;
    for (const { outcome } of report.entries) {
      switch (outcome) {
        case "created":
          totals.created += 1;
          break;
        case "cancelled":
        case "inventory_changed":
          totals.skipped += 1;
          break;
        default:
          totals.failed += 1;
      }
      if (outcome === "needs_review") needsReview = true;
    }
    return (
      needsReview === (report.runId !== null) &&
      report.created === totals.created &&
      report.failed === totals.failed &&
      report.skipped === totals.skipped
    );
  }),
);

const batchBackupReviewTarget = v.strictObject({
  runId: batchRunId,
  rowIndex: v.pipe(count, v.maxValue(MAX_BATCH_BACKUP_ROWS - 1)),
});
const batchBackupReview = v.pipe(
  v.strictObject({
    ...batchBackupReviewTarget.entries,
    kind: v.picklist([
      "snapshot_commit_uncertain",
      "metadata_rejected",
      "metadata_commit_uncertain",
    ]),
    backup: recordedPathSchema,
    recovery: v.nullable(recordedPathSchema),
    reason: recordedReasonSchema,
    reasonTruncated: v.boolean(),
  }),
  v.check(
    ({ kind, recovery }) =>
      (kind === "metadata_rejected") === (recovery === null),
  ),
);

export type BatchBackupReviewTarget = v.InferOutput<
  typeof batchBackupReviewTarget
>;
export type BatchBackupReview = v.InferOutput<typeof batchBackupReview>;

const backupRetentionReview = v.pipe(
  v.object({
    keepNewest: v.pipe(count, v.minValue(1), v.maxValue(MAX_BACKUP_RETENTION)),
    verifiedCount: count,
    retainedCount: count,
    olderCount: count,
    missingCount: count,
    unverifiedCount: count,
    rejectedMetadata: count,
    older: v.array(
      v.object({
        name: v.string(),
        timestampUtc: v.string(),
        byteLength: count,
      }),
    ),
  }),
  v.check(
    (report) =>
      report.retainedCount ===
        Math.min(report.keepNewest, report.verifiedCount) &&
      report.verifiedCount - report.retainedCount === report.olderCount &&
      report.older.length <= report.olderCount,
  ),
);

export function createBatchBackup(request: BatchBackupRequest) {
  return invokeChecked(v.nullable(batchBackupReport), "create_batch_backup", {
    scope: request.scope,
    placement: request.placement,
  });
}

export async function reviewBatchBackup(target: BatchBackupReviewTarget) {
  const request = v.safeParse(batchBackupReviewTarget, target);
  if (!request.success) throw new Error("Invalid batch backup review target.");
  const review = await invokeChecked(batchBackupReview, "review_batch_backup", {
    target: request.output,
  });
  if (
    review.runId !== request.output.runId ||
    review.rowIndex !== request.output.rowIndex
  ) {
    throw new Error("StarBank received an invalid desktop response.");
  }
  return review;
}

export function readBackupRetentionReview(target: ActiveBankTarget) {
  return invokeChecked(backupRetentionReview, "review_backup_retention", {
    target,
  });
}
