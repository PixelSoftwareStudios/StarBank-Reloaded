import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { EditorActionResult } from "starbank-ui";
import { type OpenRequest, readOpenRequests } from "./transport";

import "./open-requests.css";

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
    <aside className="open-requests" aria-label="File open requests">
      <div className="open-request-text">
        {first && (
          <p title={first.name}>
            {first.name}
            {queue.length > 1 && <span> ({queue.length - 1} more)</span>}
          </p>
        )}
        {notice && <p role="status">{notice}</p>}
      </div>
      {first ? (
        <div className="open-request-actions">
          <button
            type="button"
            className="editor-button"
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
            className="editor-button"
            disabled={busy}
            onClick={() => void dismiss(first.id)}
          >
            Dismiss
          </button>
        </div>
      ) : (
        <button
          type="button"
          className="editor-button"
          onClick={() => setNotice(undefined)}
        >
          Dismiss
        </button>
      )}
    </aside>
  );
}
