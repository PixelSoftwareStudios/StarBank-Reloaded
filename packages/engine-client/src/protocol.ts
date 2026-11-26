import * as v from "valibot";

export const EDITOR_SCHEMA_VERSION = 1;

export interface EngineSigningContext {
  readonly author: string;
  readonly player: string;
  readonly bank_name: string;
}

export interface EngineValueEdit {
  readonly id: string;
  readonly value: string;
}

export type EngineCommand =
  | {
      readonly kind: "open";
      readonly payload?: {
        readonly context?: EngineSigningContext;
        readonly discard_current?: boolean;
      };
    }
  | { readonly kind: "snapshot" }
  | { readonly kind: "raw" }
  | { readonly kind: "replace_raw" }
  | {
      readonly kind: "set_values";
      readonly payload: { readonly edits: readonly EngineValueEdit[] };
    }
  | { readonly kind: "undo" }
  | { readonly kind: "redo" }
  | { readonly kind: "reset" }
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

const signatureStatus = v.picklist([
  "unsigned",
  "malformed",
  "missing_context",
  "unsupported",
  "valid",
  "invalid",
]);

export type EngineSignatureStatus = v.InferOutput<typeof signatureStatus>;

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
  keys: v.array(engineKey),
});

const engineSnapshot = v.object({
  dirty: v.boolean(),
  can_undo: v.boolean(),
  can_redo: v.boolean(),
  signature_status: signatureStatus,
  sections: v.array(engineSection),
});

export type EngineSnapshot = v.InferOutput<typeof engineSnapshot>;

const emptyPayload = v.object({});
const resultSchema = v.variant("kind", [
  v.object({ kind: v.literal("snapshot"), payload: engineSnapshot }),
  v.object({ kind: v.literal("raw"), payload: emptyPayload }),
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
