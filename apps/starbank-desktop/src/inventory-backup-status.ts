import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { BankBackupPresence } from "starbank-ui";

interface Row {
  readonly bankId: string;
  readonly state: BankBackupPresence;
}

interface Report {
  readonly generation: string;
  readonly backupGeneration: number;
  readonly statuses: ReadonlyMap<string, BankBackupPresence>;
}

const UPDATE_DELAY_MS = 180;

export function useInventoryBackupStatuses(
  generation: string | undefined,
  backupGeneration: number,
) {
  const pending = useRef(Promise.resolve());
  const [report, setReport] = useState<Report>();

  useEffect(() => {
    if (!generation) return;
    let current = true;
    const timer = window.setTimeout(() => {
      pending.current = pending.current.then(async () => {
        if (!current) return;
        try {
          const rows = await invoke<readonly Row[]>("inventory_backup_status", {
            generation,
          });
          if (current) {
            setReport({
              generation,
              backupGeneration,
              statuses: new Map(
                rows.map(({ bankId, state }) => [bankId, state]),
              ),
            });
          }
        } catch {
          if (current) setReport(undefined);
        }
      });
    }, UPDATE_DELAY_MS);
    return () => {
      current = false;
      window.clearTimeout(timer);
    };
  }, [generation, backupGeneration]);

  return report?.generation === generation &&
    report?.backupGeneration === backupGeneration
    ? report.statuses
    : undefined;
}
