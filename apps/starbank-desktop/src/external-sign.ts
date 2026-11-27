import { invoke } from "@tauri-apps/api/core";
import type { EngineSigningContext } from "starbank-engine-client/protocol";
import type { EditorActionResult } from "starbank-ui";
import type { ManualSigningPrompt } from "./manual-signing-sheet";
import {
  checkExternalBankSignature,
  exportExternalSignedBank,
  pickExternalSigningSource,
} from "./transport";

interface ExternalSigningOptions {
  setPrompt: (prompt: ManualSigningPrompt | undefined) => void;
  errorMessage: (error: unknown) => string;
}

export async function resignExternalBank({
  setPrompt,
  errorMessage,
}: ExternalSigningOptions): Promise<EditorActionResult> {
  const source = await pickExternalSigningSource();
  if (!source) return { ok: true };

  const exportCopy = (context: EngineSigningContext | null) =>
    exportExternalSignedBank(source.id, context);

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
        check: (context) => checkExternalBankSignature(source.id, context),
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
