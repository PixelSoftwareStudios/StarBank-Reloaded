import { useEffect, useEffectEvent, useRef, useState } from "react";
import type {
  BankEditorController,
  EditorTarget,
  EditorValueEdit,
} from "./index";

const VALIDATION_DELAY_MS = 150;

type ValidateValues = BankEditorController["onValidateValues"];

export interface TableValidationRequest {
  target: EditorTarget;
  edits: readonly EditorValueEdit[];
}

type ValidationResult =
  | { kind: "checked"; issues: ReadonlyMap<string, string> }
  | { kind: "failed"; message: string };

interface CompletedValidation {
  request: TableValidationRequest;
  result: ValidationResult;
}

export function useTableDraftValidation(
  request: TableValidationRequest | undefined,
  validate: ValidateValues,
  busy: boolean,
): ValidationResult | { kind: "pending" } | undefined {
  const [completed, setCompleted] = useState<CompletedValidation>();
  const running = useRef(false);
  const hasValidator = validate !== undefined;
  const current =
    completed?.request === request ? completed?.result : undefined;

  const check = useEffectEvent(async (pending: TableValidationRequest) => {
    let result: ValidationResult;
    try {
      if (!validate)
        throw new Error("The bank engine could not check the draft.");
      const reply = await validate(pending.target, pending.edits);
      const candidates = new Set(pending.edits.map(({ id }) => id));
      const issues = new Map<string, string>();
      for (const { id, message } of reply) {
        if (!candidates.delete(id))
          throw new Error(
            "The bank engine returned an unexpected draft issue.",
          );
        issues.set(id, message);
      }
      result = { kind: "checked", issues };
    } catch (error) {
      result = {
        kind: "failed",
        message:
          error instanceof Error
            ? error.message
            : "The bank engine could not check the draft.",
      };
    }
    running.current = false;
    setCompleted({ request: pending, result });
  });

  useEffect(() => {
    if (!request || !hasValidator || busy || current || running.current) return;
    const timer = setTimeout(() => {
      running.current = true;
      void check(request);
    }, VALIDATION_DELAY_MS);
    return () => clearTimeout(timer);
  }, [request, hasValidator, busy, current, completed]);

  return request ? (current ?? { kind: "pending" }) : undefined;
}
