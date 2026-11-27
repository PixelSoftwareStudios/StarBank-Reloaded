import type {
  BankEditorDocument,
  EditorActionResult,
  EditorTarget,
} from "starbank-ui";
import {
  decodeEngineReply,
  type EngineReply,
} from "starbank-ui/engine/protocol";
import type { ManualSigningPrompt } from "./manual-signing-sheet";
import { type ActiveBankTarget, resignActiveBank } from "./transport";

interface ResignOptions {
  document?: BankEditorDocument;
  target: EditorTarget;
  activeTarget: (target: EditorTarget) => ActiveBankTarget;
  adopt: (name: string, reply: EngineReply) => Promise<void>;
  setPrompt: (prompt: ManualSigningPrompt | undefined) => void;
  errorMessage: (error: unknown) => string;
}

export async function resignOpenBank({
  document,
  target,
  activeTarget,
  adopt,
  setPrompt,
  errorMessage,
}: ResignOptions): Promise<EditorActionResult> {
  if (
    !document ||
    document.id !== target.id ||
    document.revision !== target.revision
  ) {
    return { ok: false, message: "The bank changed before re-signing." };
  }

  const native = activeTarget(target);
  const reply = await resignActiveBank(native);
  if (reply.kind === "signed") {
    await adopt(
      reply.payload.name,
      decodeEngineReply(reply.payload.control, native.requestId),
    );
    return { ok: true, message: "Re-signed the bank in the editor." };
  }
  if (reply.kind !== "manual_context_required") {
    throw new Error("The bank engine did not confirm the re-sign operation.");
  }

  return new Promise((resolve) => {
    const finish = (result: EditorActionResult) => {
      setPrompt(undefined);
      resolve(result);
    };

    setPrompt({
      name: document.name,
      sourceStatus: document.signatureStatus,
      errorMessage,
      check: async (context) => {
        const request = activeTarget(target);
        const checked = await resignActiveBank(request, {
          kind: "check",
          context,
        });
        if (checked.kind !== "checked") {
          throw new Error("The bank engine did not return a signature check.");
        }
        return checked.payload;
      },
      apply: async (context) => {
        const request = activeTarget(target);
        const signed = await resignActiveBank(request, {
          kind: "apply",
          context,
        });
        if (signed.kind !== "signed") {
          throw new Error(
            "The bank engine did not confirm the re-sign operation.",
          );
        }
        await adopt(
          signed.payload.name,
          decodeEngineReply(signed.payload.control, request.requestId),
        );
        finish({
          ok: true,
          message: "Re-signed the bank for the supplied context.",
        });
      },
      dismiss: () => finish({ ok: true }),
    });
  });
}
