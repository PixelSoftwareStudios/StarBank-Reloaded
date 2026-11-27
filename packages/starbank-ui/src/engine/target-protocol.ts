import * as v from "valibot";

const MAX_DOCUMENT_ID_BYTES = 128;
const maxRevision = "18446744073709551615";

export const engineRevisionSchema = v.pipe(
  v.string(),
  v.regex(/^(0|[1-9][0-9]{0,19})$/),
  v.check((value) => value.length < maxRevision.length || value <= maxRevision),
);

export const engineTargetSchema = v.strictObject({
  document_id: v.pipe(
    v.string(),
    v.nonEmpty(),
    v.maxLength(MAX_DOCUMENT_ID_BYTES),
    v.regex(/^[^\p{Cc}\p{Cs}]+$/u),
    v.maxBytes(MAX_DOCUMENT_ID_BYTES),
  ),
  expected_revision: engineRevisionSchema,
});
