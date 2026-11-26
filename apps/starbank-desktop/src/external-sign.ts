import { invoke } from "@tauri-apps/api/core";
import type {
  EngineSignatureStatus,
  EngineSigningContext,
} from "starbank-engine-client/protocol";
import type { EditorActionResult } from "starbank-ui";
import type { ManualSigningPrompt } from "./manual-signing-sheet";

interface ExternalSigningSource {
  readonly id: string;
  readonly name: string;
  readonly requires_context: boolean;
  readonly signature_status: EngineSignatureStatus;
}

interface ExternalSigningOptions {
  setPrompt: (prompt: ManualSigningPrompt | undefined) => void;
  errorMessage: (error: unknown) => string;
}

export async function resignExternalBank({
  setPrompt,
  errorMessage,
}: ExternalSigningOptions): Promise<EditorActionResult> {
  const source = await invoke<ExternalSigningSource | null>(
    "pick_external_bank_to_resign",
  );
  if (!source) return { ok: true };

  const exportCopy = (context: EngineSigningContext | null) =>
    invoke<string | null>("export_external_signed_bank", {
      id: source.id,
      context,
    });

  try {
    if (!source.requires_context) return exportResult(await exportCopy(null));

    return await new Promise((resolve) => {
      function finish(result: EditorActionResult) {
        setPrompt(undefined);
        resolve(result);
      }

      setPrompt({
        name: source.name,
        sourceStatus: source.signature_status,
        purpose: "signed_copy",
        errorMessage,
        check: (context) =>
          invoke<Exclude<EngineSignatureStatus, "missing_context">>(
            "check_external_bank_signature",
            { id: source.id, context },
          ),
        apply: async (context) => {
          const name = await exportCopy(context);
          if (name !== null) finish(exportResult(name));
        },
        dismiss: () => finish({ ok: true }),
      });
    });
  } finally {
    await invoke("discard_external_signing_source", { id: source.id }).catch(
      () => {},
    );
  }
}

function exportResult(name: string | null): EditorActionResult {
  return name === null
    ? { ok: true }
    : { ok: true, message: `Saved verified signed copy ${name}.` };
}
