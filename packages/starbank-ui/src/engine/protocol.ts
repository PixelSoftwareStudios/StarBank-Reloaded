import * as v from "valibot";
import {
  type EngineProfileCommand,
  profilePreviewSchema,
  profileSchema,
} from "./profile-protocol";
import { type EngineSigningContext, signatureStatus } from "./signing-protocol";

export type {
  EngineProfile,
  EngineProfileEdit,
  EngineProfileIdentity,
  EngineProfileInput,
  EngineProfileLoad,
  EngineProfilePreview,
} from "./profile-protocol";
export {
  bankFilenameStem,
  MAX_PROFILE_BYTES,
  profileLoadForFile,
} from "./profile-protocol";

export const EDITOR_SCHEMA_VERSION = 1;

export type {
  EngineSignatureStatus,
  EngineSigningContext,
} from "./signing-protocol";

export interface EngineValueEdit {
  readonly id: string;
  readonly value: string;
}

const engineValueValidationIssue = v.object({
  id: v.string(),
  code: v.picklist([
    "unknown_value",
    "duplicate_value",
    "opaque_value",
    "value_too_large",
    "invalid_integer",
    "invalid_fixed",
    "unsupported_kind",
  ]),
  message: v.string(),
});

export type EngineValueValidationIssue = v.InferOutput<
  typeof engineValueValidationIssue
>;

export const STRUCTURED_VALUE_KINDS = [
  "int",
  "fixed",
  "string",
  "text",
] as const;
export type EngineStructuredValueKind = (typeof STRUCTURED_VALUE_KINDS)[number];

export type EngineStructureChange =
  | { readonly kind: "add_section"; readonly name: string }
  | {
      readonly kind: "remove_section" | "remove_key" | "remove_value";
      readonly id: string;
    }
  | {
      readonly kind: "rename_section" | "rename_key" | "rename_value";
      readonly id: string;
      readonly name: string;
    }
  | {
      readonly kind: "add_key";
      readonly section_id: string;
      readonly name: string;
    }
  | {
      readonly kind: "add_value";
      readonly key_id: string;
      readonly name: string;
      readonly value_kind: EngineStructuredValueKind;
      readonly value: string;
    }
  | {
      readonly kind: "set_value_type";
      readonly id: string;
      readonly value_kind: EngineStructuredValueKind;
      readonly value: string;
    };

const occurrenceKind = v.picklist(["section", "key", "value"]);

export interface EngineRevertTarget {
  readonly kind: v.InferOutput<typeof occurrenceKind>;
  readonly id: string;
}

export type EngineCommand =
  | EngineProfileCommand
  | {
      readonly kind: "open";
      readonly payload?: {
        readonly context?: EngineSigningContext;
        readonly discard_current?: boolean;
      };
    }
  | { readonly kind: "snapshot" }
  | { readonly kind: "raw" }
  | { readonly kind: "raw_text" }
  | { readonly kind: "opening_raw" }
  | { readonly kind: "replace_raw" }
  | { readonly kind: "replace_raw_text" }
  | {
      readonly kind: "set_values";
      readonly payload: { readonly edits: readonly EngineValueEdit[] };
    }
  | {
      readonly kind: "validate_values";
      readonly payload: { readonly edits: readonly EngineValueEdit[] };
    }
  | { readonly kind: "structure"; readonly payload: EngineStructureChange }
  | { readonly kind: "diff" }
  | { readonly kind: "revert"; readonly payload: EngineRevertTarget }
  | { readonly kind: "undo" }
  | { readonly kind: "redo" }
  | { readonly kind: "reset" }
  | { readonly kind: "revert_to_saved" }
  | { readonly kind: "close"; readonly payload?: { readonly discard: boolean } }
  | {
      readonly kind: "prepare";
      readonly payload?: { readonly context?: EngineSigningContext };
    };

export interface EngineTarget {
  readonly document_id: string;
  readonly expected_revision: string;
}

export interface EngineRequest {
  readonly schema_version: typeof EDITOR_SCHEMA_VERSION;
  readonly request_id: string;
  readonly document_id?: string;
  readonly expected_revision?: string;
  readonly command: EngineCommand;
}

const engineValue = v.object({
  id: v.string(),
  name: v.string(),
  kind: v.nullable(v.string()),
  value: v.nullable(v.string()),
  editable: v.boolean(),
});

const engineKey = v.object({
  id: v.string(),
  name: v.nullable(v.string()),
  values: v.array(engineValue),
});

const engineSection = v.object({
  id: v.string(),
  name: v.nullable(v.string()),
  dirty: v.optional(v.boolean()),
  keys: v.array(engineKey),
});

const engineSnapshot = v.object({
  dirty: v.boolean(),
  dirty_section_count: v.optional(
    v.pipe(v.number(), v.safeInteger(), v.minValue(0)),
  ),
  can_undo: v.boolean(),
  can_redo: v.boolean(),
  signature_status: signatureStatus,
  sections: v.array(engineSection),
});

export type EngineSnapshot = v.InferOutput<typeof engineSnapshot>;

const diffOccurrence = v.object({
  parent: v.nullable(v.string()),
  name: v.nullable(v.string()),
  value: v.nullable(v.object({ kind: v.string(), text: v.string() })),
});

const diffChange = v.variant("kind", [
  v.object({ kind: v.literal("added"), payload: diffOccurrence }),
  v.object({ kind: v.literal("removed"), payload: diffOccurrence }),
  v.object({
    kind: v.literal("changed"),
    payload: v.object({ before: diffOccurrence, after: diffOccurrence }),
  }),
]);

const engineOccurrenceDiff = v.object({
  id: v.string(),
  kind: occurrenceKind,
  change: diffChange,
});

const engineDiff = v.object({
  source_changed: v.boolean(),
  changes: v.array(engineOccurrenceDiff),
});

export type EngineDiff = v.InferOutput<typeof engineDiff>;
export type EngineOccurrenceDiff = v.InferOutput<typeof engineOccurrenceDiff>;

const sourceEncoding = v.picklist(["utf-8", "utf-16le", "utf-16be"]);

export type EngineSourceEncoding = v.InferOutput<typeof sourceEncoding>;

const emptyPayload = v.object({});
const resultSchema = v.variant("kind", [
  v.object({ kind: v.literal("snapshot"), payload: engineSnapshot }),
  v.object({
    kind: v.literal("value_validation"),
    payload: v.object({ issues: v.array(engineValueValidationIssue) }),
  }),
  v.object({ kind: v.literal("profile"), payload: profileSchema }),
  v.object({
    kind: v.literal("profile_preview"),
    payload: profilePreviewSchema,
  }),
  v.object({ kind: v.literal("raw"), payload: emptyPayload }),
  v.object({
    kind: v.literal("raw_text"),
    payload: v.object({ source_encoding: sourceEncoding }),
  }),
  v.object({ kind: v.literal("diff"), payload: engineDiff }),
  v.object({
    kind: v.literal("prepared"),
    payload: v.object({ signature_status: signatureStatus }),
  }),
  v.object({ kind: v.literal("closed"), payload: emptyPayload }),
  v.object({
    kind: v.literal("error"),
    payload: v.object({ code: v.string(), message: v.string() }),
  }),
]);

const maxRevision = "18446744073709551615";
const revision = v.pipe(
  v.string(),
  v.regex(/^(0|[1-9][0-9]{0,19})$/),
  v.check((value) => value.length < maxRevision.length || value <= maxRevision),
);

const replySchema = v.object({
  schema_version: v.literal(EDITOR_SCHEMA_VERSION),
  request_id: v.nullable(v.string()),
  document_id: v.nullable(v.string()),
  revision: v.nullable(revision),
  result: resultSchema,
});

export type EngineReply = v.InferOutput<typeof replySchema>;

export function engineRequest(
  requestId: string,
  command: EngineCommand,
  target?: EngineTarget,
): EngineRequest {
  return {
    schema_version: EDITOR_SCHEMA_VERSION,
    request_id: requestId,
    ...target,
    command,
  };
}

/** Decodes a response for the command that produced it. Revisions stay exact decimal strings. */
export function decodeEngineReply(
  control: string,
  requestId: string,
): EngineReply {
  let message: unknown;
  try {
    message = JSON.parse(control);
  } catch {
    throw new Error("The bank engine sent an unreadable reply.");
  }
  const reply = v.safeParse(replySchema, message);
  if (
    !reply.success ||
    reply.output.request_id !== requestId ||
    (reply.output.result.kind !== "error" &&
      (reply.output.document_id === null || reply.output.revision === null))
  ) {
    throw new Error("The bank engine sent an invalid reply.");
  }
  return reply.output;
}
