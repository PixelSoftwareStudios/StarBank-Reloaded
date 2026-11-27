import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import {
  confirmWindowClose,
  decodeCloseProbe,
  MAX_NATIVE_U32,
} from "./transport";

const REQUEST_EVENT = "bank-close-requested";
const BUSY_NOTICE =
  "Wait for the current bank action to finish before closing.";

interface CloseOptions {
  busy: boolean;
  save?: () => Promise<boolean>;
  onDraft: (hasDraft: boolean) => void;
  onNotice: (message: string) => void;
}

interface DraftState {
  generation: number;
  hasDraft: boolean;
}

export function useNativeCloseGuard(options: CloseOptions) {
  const current = useRef(options);
  const draft = useRef<DraftState>({ generation: 0, hasDraft: false });
  const running = useRef(false);
  const [pending, setPending] = useState(false);

  useLayoutEffect(() => {
    current.current = options;
  }, [options]);

  const reportDraft = useCallback((hasDraft: boolean) => {
    draft.current = {
      generation: Math.min(draft.current.generation + 1, MAX_NATIVE_U32),
      hasDraft,
    };
    current.current.onDraft(hasDraft);
  }, []);

  useEffect(() => {
    let mounted = true;
    const listening = listen<unknown>(REQUEST_EVENT, ({ payload }) => {
      if (!mounted || running.current) return;
      const probe = decodeCloseProbe(payload);
      if (!probe) return;
      if (current.current.busy) {
        current.current.onNotice(BUSY_NOTICE);
        return;
      }
      const { ticket } = probe;
      const observed = draft.current;
      const save = current.current.save;
      running.current = true;
      setPending(true);

      void (async () => {
        const complete = (proceed: boolean) =>
          invoke("complete_window_close", {
            ticket,
            draftGeneration: draft.current.generation,
            proceed,
          });
        let approved = false;
        try {
          const choice = await confirmWindowClose({
            ticket,
            draftGeneration: observed.generation,
            hasDraft: observed.hasDraft,
            canSave: save !== undefined,
          });
          if (choice !== "discard" && choice !== "save") return;
          approved = true;
          const currentDraft = () =>
            mounted &&
            draft.current === observed &&
            observed.generation < MAX_NATIVE_U32;
          const busyBeforeCompletion = current.current.busy;
          const canProceed =
            currentDraft() &&
            !busyBeforeCompletion &&
            (choice === "discard" || (save !== undefined && (await save())));
          const proceed = canProceed && currentDraft();
          await complete(proceed);
          approved = false;
          if (!proceed && mounted) {
            if (busyBeforeCompletion) current.current.onNotice(BUSY_NOTICE);
            else if (draft.current !== observed) {
              current.current.onNotice(
                "The draft changed before the window could close.",
              );
            }
          }
        } catch {
          if (approved) await complete(false).catch(() => {});
          if (mounted) {
            current.current.onNotice(
              "The window stayed open to preserve your changes.",
            );
          }
        } finally {
          running.current = false;
          if (mounted) setPending(false);
        }
      })();
    });

    void listening.catch(() => {
      if (mounted)
        current.current.onNotice("Window close protection could not start.");
    });
    return () => {
      mounted = false;
      void listening.then(
        (unlisten) => unlisten(),
        () => {},
      );
    };
  }, []);

  return { pending, reportDraft };
}
