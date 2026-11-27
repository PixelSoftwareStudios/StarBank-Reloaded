import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { EditorActionResult } from "starbank-ui";
import { buttonClasses } from "starbank-ui/controls";
import { type OpenRequest, readOpenRequests } from "./transport";

export { openRequestedBank } from "./transport";

const QUEUE_CHANGED = "bank-open-requests";

export function OpenRequests({
  busy,
  hasDraft,
  onOpen,
}: {
  busy: boolean;
  hasDraft: boolean;
  onOpen: (id: number) => Promise<EditorActionResult>;
}) {
  const [queue, setQueue] = useState<readonly OpenRequest[]>([]);
  const [notice, setNotice] = useState<string>();

  useEffect(() => {
    let stopped = false;
    let sequence = 0;
    let unlisten: (() => void) | undefined;

    async function refresh() {
      const current = ++sequence;
      try {
        const requests = await readOpenRequests();
        if (!stopped && current === sequence) setQueue(requests);
      } catch {
        if (!stopped && current === sequence)
          setNotice("Pending file requests could not be read.");
      }
    }

    void listen(QUEUE_CHANGED, () => void refresh()).then(
      (cancel) => {
        if (stopped) cancel();
        else {
          unlisten = cancel;
          void refresh();
        }
      },
      () => void refresh(),
    );
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, []);

  async function dismiss(id: number) {
    setNotice(undefined);
    try {
      await invoke("dismiss_open_request", { id });
      setQueue((current) => current.filter((request) => request.id !== id));
    } catch {
      setNotice("The file request could not be dismissed.");
    }
  }

  const first = queue[0];
  if (!first && !notice) return null;
  return (
    <aside
      className="open-requests flex items-center justify-between gap-2 border-b border-line px-2 py-[0.35rem] text-[0.8rem]"
      aria-label="File open requests"
    >
      <div className="min-w-0">
        {first && (
          <p title={first.name} className="m-0 truncate">
            {first.name}
            {queue.length > 1 && <span> ({queue.length - 1} more)</span>}
          </p>
        )}
        {notice && (
          <p role="status" className="m-0 truncate">
            {notice}
          </p>
        )}
      </div>
      {first ? (
        <div className="flex flex-none gap-[0.3rem]">
          <button
            type="button"
            className={buttonClasses()}
            disabled={busy || hasDraft}
            title={
              hasDraft ? "Apply or discard the current draft first" : undefined
            }
            onClick={() => {
              setNotice(undefined);
              void onOpen(first.id).then((result) => {
                if (!result.ok) setNotice(result.message);
              });
            }}
          >
            Open
          </button>
          <button
            type="button"
            className={buttonClasses()}
            disabled={busy}
            onClick={() => void dismiss(first.id)}
          >
            Dismiss
          </button>
        </div>
      ) : (
        <button
          type="button"
          className={buttonClasses()}
          onClick={() => setNotice(undefined)}
        >
          Dismiss
        </button>
      )}
    </aside>
  );
}
