import * as v from "valibot";

export interface EngineSigningContext {
  readonly author: string;
  readonly player: string;
  readonly bank_name: string;
}

export const signatureStatus = v.picklist([
  "unsigned",
  "malformed",
  "missing_context",
  "unsupported",
  "valid",
  "invalid",
]);

export type EngineSignatureStatus = v.InferOutput<typeof signatureStatus>;
