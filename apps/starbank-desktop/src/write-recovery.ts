import { engineTargetSchema } from "starbank-ui/engine/target-protocol";
import * as v from "valibot";

const RECOVERY_ERROR = "recovery_required";
const MAX_DISPLAY_PATH_BYTES = 4_096;
const MAX_REASON_BYTES = 512;
const UNPAIRED_SURROGATE = /[\uD800-\uDFFF]/u;

function boundedText(maximum: number) {
  return v.pipe(
    v.string(),
    v.maxLength(maximum),
    v.check((value) => !UNPAIRED_SURROGATE.test(value)),
    v.maxBytes(maximum),
  );
}

export const recordedPathSchema = v.strictObject({
  display: boundedText(MAX_DISPLAY_PATH_BYTES),
  truncated: v.boolean(),
  lossy: v.boolean(),
});

export const recordedReasonSchema = boundedText(MAX_REASON_BYTES);

const recoveryLocation = v.variant("kind", [
  v.strictObject({ kind: v.literal("recorded"), path: recordedPathSchema }),
  v.strictObject({ kind: v.literal("unknown") }),
]);

const backupEvidence = v.variant("kind", [
  v.strictObject({ kind: v.literal("verified"), path: recordedPathSchema }),
  v.strictObject({ kind: v.literal("unverified"), path: recordedPathSchema }),
  v.strictObject({ kind: v.literal("not_created") }),
]);

const recoveryReceipt = v.pipe(
  v.strictObject({
    operation: v.picklist([
      "save_overwrite",
      "save_as",
      "restore",
      "export_copy",
      "backup_copy",
      "external_signed_copy",
    ]),
    outcome: v.picklist([
      "commit_uncertain",
      "live_unchanged",
      "verified_needs_reopen",
    ]),
    affectsActive: v.boolean(),
    destination: recordedPathSchema,
    recovery: recoveryLocation,
    backup: backupEvidence,
    reason: recordedReasonSchema,
    reasonTruncated: v.boolean(),
    editorTarget: v.exactOptional(engineTargetSchema),
  }),
  v.check(({ operation, outcome, affectsActive, backup, editorTarget }) => {
    const writesActive =
      operation === "save_overwrite" || operation === "restore";
    const verifiedWrite = outcome === "verified_needs_reopen";
    if (editorTarget !== undefined && !verifiedWrite) return false;
    if (verifiedWrite && !writesActive && operation !== "save_as") return false;
    if (outcome === "live_unchanged") {
      return writesActive && !affectsActive && backup.kind === "unverified";
    }
    return writesActive
      ? affectsActive &&
          (backup.kind === "verified" ||
            (operation === "save_overwrite" && backup.kind === "not_created"))
      : !affectsActive && backup.kind === "not_created";
  }),
);

const recoveryError = v.strictObject({ [RECOVERY_ERROR]: recoveryReceipt });

export type WriteRecoveryPath = v.InferOutput<typeof recordedPathSchema>;
export type WriteRecoveryReceipt = v.InferOutput<typeof recoveryReceipt>;

export type WriteRecoveryEvidence =
  | { readonly kind: "receipt"; readonly receipt: WriteRecoveryReceipt }
  | { readonly kind: "invalid_receipt" };

export function parseWriteRecoveryError(
  error: unknown,
): WriteRecoveryEvidence | undefined {
  if (error === RECOVERY_ERROR) return { kind: "invalid_receipt" };
  if (
    typeof error !== "object" ||
    error === null ||
    !Object.hasOwn(error, RECOVERY_ERROR)
  ) {
    return undefined;
  }

  const result = v.safeParse(recoveryError, error, {
    abortEarly: true,
    abortPipeEarly: true,
  });
  return result.success
    ? { kind: "receipt", receipt: result.output[RECOVERY_ERROR] }
    : { kind: "invalid_receipt" };
}
