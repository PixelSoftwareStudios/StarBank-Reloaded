import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { EditorTarget } from "starbank-ui";
import type { ActiveBankTarget } from "./transport";

export interface ActiveBackupStatus {
  readonly state: "never_backed_up" | "recorded" | "unknown";
  readonly recordCount: number;
  readonly rejectedMetadata: number;
  readonly latestTimestampUtc: string | null;
  readonly incomplete: boolean;
}

function readActiveBackupStatus(
  target: ActiveBankTarget,
): Promise<ActiveBackupStatus> {
  return invoke<ActiveBackupStatus>("active_backup_status", { target });
}

function describeBackupStatus(status: ActiveBackupStatus): string {
  if (status.state === "never_backed_up") return "No backups recorded";
  if (status.state === "unknown") return "Backup history unavailable";
  const noun = status.recordCount === 1 ? "backup" : "backups";
  return status.incomplete
    ? `At least ${status.recordCount} ${noun} recorded`
    : `${status.recordCount} ${noun} recorded`;
}

export function useActiveBackupStatus(
  target: EditorTarget | undefined,
  backupGeneration: number,
): string | undefined {
  const documentId = target?.id;
  const revision = target?.revision;
  const [entry, setEntry] = useState<{ documentId: string; label: string }>();

  useEffect(() => {
    if (!documentId || revision === undefined) return;
    let active = true;
    const timer = window.setTimeout(() => {
      void readActiveBackupStatus({
        documentId,
        revision,
        requestId: `backup-status-${revision}-${backupGeneration}`,
      }).then(
        (status) => {
          if (active)
            setEntry({ documentId, label: describeBackupStatus(status) });
        },
        () => {
          if (active)
            setEntry({ documentId, label: "Backup history unavailable" });
        },
      );
    }, 180);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [documentId, revision, backupGeneration]);

  return entry?.documentId === documentId ? entry?.label : undefined;
}
