import * as v from "valibot";

export const MAX_PROFILE_BYTES = 64 * 1024;
const BANK_EXTENSION = ".sc2bank";

export interface EngineProfileIdentity {
  readonly author: string;
  readonly bank_name: string;
}

export type EngineProfileInput =
  | { readonly kind: "text"; readonly value: string }
  | { readonly kind: "checked"; readonly value: boolean };

export interface EngineProfileEdit {
  readonly profile_id: string;
  readonly field_id: string;
  readonly input: EngineProfileInput;
}

export type EngineProfileLoad =
  | { readonly format: "json"; readonly identity?: EngineProfileIdentity }
  | {
      readonly format: "legacy_yaml";
      readonly profile_id: string;
      readonly identity?: EngineProfileIdentity;
    };

export function bankFilenameStem(filename: string): string | undefined {
  if (!filename.toLowerCase().endsWith(BANK_EXTENSION)) return undefined;
  return filename.slice(0, -BANK_EXTENSION.length) || undefined;
}

export function profileLoadForFile(
  file: { name: string; size: number },
  bankName: string,
  author?: string,
): EngineProfileLoad {
  if (file.size > MAX_PROFILE_BYTES) {
    throw new RangeError("This profile exceeds the engine's file size limit.");
  }
  const stem = bankFilenameStem(bankName);
  if (author && !stem) {
    throw new Error(
      "Open the bank with its original .SC2Bank filename to set a profile identity.",
    );
  }
  const identity = author && stem ? { author, bank_name: stem } : undefined;
  const dot = file.name.lastIndexOf(".");
  const extension = file.name.slice(dot).toLowerCase();
  if (extension === ".json") return { format: "json", identity };
  if (extension === ".yaml" || extension === ".yml") {
    return {
      format: "legacy_yaml",
      profile_id: file.name.slice(0, dot),
      identity,
    };
  }
  throw new Error("Choose a JSON or legacy YAML profile.");
}

export type EngineProfileCommand =
  | { readonly kind: "profile_snapshot" }
  | { readonly kind: "profile_clear" }
  | { readonly kind: "profile_load"; readonly payload: EngineProfileLoad }
  | { readonly kind: "profile_preview"; readonly payload: EngineProfileEdit }
  | {
      readonly kind: "profile_apply";
      readonly payload: { readonly preview_id: string };
    };

const profileField = v.pipe(
  v.object({
    id: v.string(),
    target_id: v.string(),
    label: v.string(),
    description: v.nullable(v.string()),
    kind: v.picklist(["number", "checkbox", "text"]),
    value: v.string(),
    checked: v.nullable(v.boolean()),
    read_only: v.boolean(),
  }),
  v.check(({ kind, checked }) => (kind === "checkbox") === (checked !== null)),
);

const profileGroup = v.object({
  label: v.string(),
  description: v.nullable(v.string()),
  fields: v.array(profileField),
});

export const profileSchema = v.variant("status", [
  v.object({ status: v.literal("unloaded") }),
  v.object({ status: v.literal("missing_context") }),
  v.object({ status: v.literal("unrecognized") }),
  v.object({ status: v.literal("incompatible") }),
  v.object({
    status: v.literal("ambiguous"),
    profile_ids: v.array(v.string()),
  }),
  v.object({
    status: v.literal("matched"),
    profile_id: v.string(),
    confidence: v.picklist([
      "legacy",
      "identity_and_schema",
      "version",
      "fingerprint",
      "version_and_fingerprint",
    ]),
    groups: v.array(profileGroup),
  }),
]);

export const profilePreviewSchema = v.object({
  preview_id: v.pipe(v.string(), v.regex(/^[1-9]\d{0,19}$/)),
  changes: v.array(
    v.object({
      field_id: v.string(),
      id: v.string(),
      previous: v.string(),
      value: v.string(),
    }),
  ),
});

export type EngineProfile = v.InferOutput<typeof profileSchema>;
export type EngineProfilePreview = v.InferOutput<typeof profilePreviewSchema>;
