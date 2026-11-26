import type { EditorActionResult, EditorTarget } from "./editor-model";

export interface EditorProfileField {
  readonly id: string;
  readonly target_id: string;
  readonly label: string;
  readonly description: string | null;
  readonly kind: "number" | "checkbox" | "text";
  readonly value: string;
  readonly checked: boolean | null;
  readonly read_only: boolean;
}

export interface EditorProfileGroup {
  readonly label: string;
  readonly description: string | null;
  readonly fields: readonly EditorProfileField[];
}

export interface MatchedEditorProfile {
  readonly status: "matched";
  readonly profile_id: string;
  readonly groups: readonly EditorProfileGroup[];
}

export type EditorProfile =
  | { readonly status: "unloaded" }
  | { readonly status: "missing_context" }
  | { readonly status: "unrecognized" }
  | { readonly status: "incompatible" }
  | { readonly status: "ambiguous"; readonly profile_ids: readonly string[] }
  | MatchedEditorProfile;

export type EditorProfileInput =
  | { readonly kind: "text"; readonly value: string }
  | { readonly kind: "checked"; readonly value: boolean };

export interface EditorProfileEdit {
  readonly profile_id: string;
  readonly field_id: string;
  readonly input: EditorProfileInput;
}

export interface EditorProfilePreview {
  readonly preview_id: string;
  readonly changes: readonly {
    readonly field_id: string;
    readonly id: string;
    readonly previous: string;
    readonly value: string;
  }[];
}

export interface EditorProfileController {
  readonly current: EditorProfile;
  readonly requireAuthor?: boolean;
  readonly onLoad: (
    target: EditorTarget,
    file: File,
    author?: string,
  ) => Promise<EditorActionResult>;
  readonly onClear: (target: EditorTarget) => Promise<EditorActionResult>;
  readonly onPreview: (
    target: EditorTarget,
    edit: EditorProfileEdit,
  ) => Promise<EditorProfilePreview>;
  readonly onApply: (
    target: EditorTarget,
    previewId: string,
  ) => Promise<EditorActionResult>;
}

export interface EditorProfileDraft extends EditorProfileEdit {
  readonly revision: string;
  readonly preview?: EditorProfilePreview;
}

export function profileStatus(current?: EditorProfile): string | undefined {
  switch (current?.status) {
    case "missing_context":
      return "Profile matching requires the map author and bank name.";
    case "unrecognized":
      return "The loaded profile belongs to a different bank identity.";
    case "incompatible":
      return "The profile requires a different bank layout or verified map evidence.";
    case "ambiguous":
      return "Several profiles match this bank. Use Table View.";
    default:
      return undefined;
  }
}
