import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MapInventory } from "starbank-ui";
import { useInventoryBackupStatuses } from "./inventory-backup-status";
import {
  fileDialogReply,
  type InventoryProgress,
  type InventoryQuery,
  type InventorySnapshot,
  invokeChecked,
  readInventory,
} from "./transport";

const PROGRESS_LABELS: Record<InventoryProgress["stage"], string> = {
  locating_folder: "Finding the bank folder…",
  enumerating_banks: "Scanning for bank files…",
  indexing_maps: "Finding cached map archives…",
  inspecting_archives: "Inspecting cached map metadata…",
};

const FAILURE_TEXT = {
  invalid_folder: "Select a StarCraft II folder or a folder containing banks.",
  unreadable: "The StarCraft II folder could not be read.",
  too_many_entries: "The folder exceeds the bank scan limit.",
  too_many_matches: "The folder exceeds the map association limit.",
  identifier_conflict: "The inventory could not distinguish two entries.",
  configuration_unavailable:
    "The saved bank folder could not be read or updated.",
};
const INVENTORY_FAILURE = "The bank inventory could not be loaded.";

export function openInventoryBank(id: string, control: string) {
  return invokeChecked(fileDialogReply, "open_bank_id", { id, control });
}

export function useInventory({
  onOpenBank,
  disabled,
  selectedBankId,
  backupGeneration = 0,
}: {
  onOpenBank: (id: string) => void;
  disabled: boolean;
  selectedBankId?: string;
  backupGeneration?: number;
}): MapInventory {
  const [snapshot, setSnapshot] = useState<InventorySnapshot>();
  const [query, setQuery] = useState<InventoryQuery>();
  const [message, setMessage] = useState<string>();
  const [progress, setProgress] = useState<InventoryProgress>();
  const [cancelled, setCancelled] = useState(false);
  const [map, setMap] = useState<string | null>(null);
  const active = useRef(false);

  const run = useCallback(async (command: InventoryQuery) => {
    if (active.current) return;
    active.current = true;
    setMessage(undefined);
    setCancelled(false);
    setQuery(command);
    setProgress(undefined);
    try {
      const result = await readInventory(command, setProgress);
      if (result) {
        setSnapshot(result);
        if (
          command === "rebuild_map_index" &&
          result.status === "ready" &&
          !result.reason
        ) {
          setMessage("Map index rebuilt.");
        }
      }
    } catch (error) {
      if (error === "cancelled") {
        setCancelled(true);
        if (command === "rebuild_map_index")
          setMessage("Map index rebuild canceled.");
      } else {
        setMessage(
          error === "busy"
            ? "A scan or native file dialog is already open."
            : error === "map_index_rebuild_failed"
              ? "The map index could not be rebuilt."
              : INVENTORY_FAILURE,
        );
      }
    } finally {
      active.current = false;
      setQuery(undefined);
      setProgress(undefined);
    }
  }, []);

  useEffect(() => {
    void run("refresh_inventory");
  }, [run]);

  const generation =
    !query && !cancelled && snapshot?.status === "ready"
      ? snapshot.generation
      : undefined;
  const backupStatuses = useInventoryBackupStatuses(
    generation,
    backupGeneration,
  );

  const controls = {
    onRefresh: () => void run("refresh_inventory"),
    onRebuildMapIndex:
      snapshot?.status === "ready" && snapshot.canRebuildMapIndex
        ? () => void run("rebuild_map_index")
        : undefined,
    onSelectDirectory: () => void run("select_inventory_folder"),
    onUseAutomatic: snapshot?.manual
      ? () => void run("use_automatic_inventory")
      : undefined,
    onCancel: progress
      ? () => {
          void invoke("cancel_inventory", {
            generation: progress.generation,
          }).catch(() => setMessage("The scan could not be canceled."));
        }
      : undefined,
  };
  const rootName = snapshot?.rootName;
  const failure = snapshot?.reason ? FAILURE_TEXT[snapshot.reason] : undefined;
  if (query) {
    const rebuilding = query === "rebuild_map_index";
    let currentStage = "Starting inventory scan…";
    if (rebuilding) currentStage = "Rebuilding map index…";
    else if (query === "select_inventory_folder")
      currentStage = "Choose a bank folder…";
    if (progress && (!rebuilding || progress.stage !== "inspecting_archives")) {
      currentStage = PROGRESS_LABELS[progress.stage];
    }
    return {
      status: "scanning",
      controls,
      rootName,
      message: message ?? currentStage,
      ...(progress?.stage === "inspecting_archives"
        ? { scanned: progress.completed, total: progress.total }
        : {}),
    };
  }
  if (cancelled) return { status: "cancelled", controls, rootName, message };
  if (snapshot?.status === "ready") {
    return {
      status: "ready",
      maps: snapshot.maps,
      selectedMapId: snapshot.maps.some(({ id }) => id === map) ? map : null,
      onSelectMap: setMap,
      bankBrowser: {
        banks: snapshot.banks,
        onOpenBank,
        selectedBankId,
        backupStatuses,
        disabled,
      },
      incomplete: snapshot.incomplete,
      message: message ?? failure,
      controls,
      rootName,
    };
  }
  if (message || snapshot?.status === "failed") {
    return {
      status: "failed",
      message: message ?? failure ?? INVENTORY_FAILURE,
      controls,
      rootName,
    };
  }
  return {
    status: snapshot?.status ?? "unavailable",
    controls,
    rootName,
  };
}
