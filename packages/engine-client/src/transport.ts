import * as v from "valibot";

const positiveSafeInteger = v.pipe(v.number(), v.safeInteger(), v.minValue(1));
const transferableBytes = v.optional(v.instance(ArrayBuffer));
const textEncoder = new TextEncoder();
const MAX_UTF8_BYTES_PER_CODE_UNIT = 3;

export const TRANSPORT_VERSION = 1;

export const engineInfoSchema = v.object({
  schemaVersion: positiveSafeInteger,
  maxBankBytes: positiveSafeInteger,
  maxControlBytes: positiveSafeInteger,
});

export type EngineInfo = v.InferOutput<typeof engineInfoSchema>;

export const requestSchema = v.object({
  id: positiveSafeInteger,
  control: v.string(),
  bytes: transferableBytes,
});

export type WorkerRequest = v.InferOutput<typeof requestSchema>;

export const replySchema = v.variant("kind", [
  v.object({
    kind: v.literal("ready"),
    version: v.literal(TRANSPORT_VERSION),
    info: engineInfoSchema,
  }),
  v.object({
    kind: v.literal("response"),
    id: positiveSafeInteger,
    control: v.string(),
    bytes: transferableBytes,
  }),
  v.object({ kind: v.literal("failed") }),
]);

export type WorkerReply = v.InferOutput<typeof replySchema>;

export function withinRequestLimits(
  request: WorkerRequest,
  info: EngineInfo,
): boolean {
  if (
    request.control.length > info.maxControlBytes ||
    (request.bytes?.byteLength ?? 0) > info.maxBankBytes
  ) {
    return false;
  }
  return (
    request.control.length <=
      info.maxControlBytes / MAX_UTF8_BYTES_PER_CODE_UNIT ||
    textEncoder.encode(request.control).byteLength <= info.maxControlBytes
  );
}
