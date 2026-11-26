import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import type { MapInventory, MapListItem } from "starbank-ui";
import type { FileDialogReply } from "./transport";

interface InventorySnapshot {
  status: "ready" | "root-not-found" | "manual-required" | "failed";
  reason: keyof typeof FAILURE_TEXT | null;
  maps: readonly MapListItem[];
  banks: readonly {
    id: string;
    name: string;
    scopeId: string | null;
    playerHandle: string | null;
    authorHandle: string | null;
    mapIds: readonly string[];
  }[];
  rootName: string | null;
  manual: boolean;
  incomplete: boolean;
}

const FAILURE_TEXT = {
  invalid_folder: "Select a StarCraft II folder or a folder containing banks.",
  unreadable: "The StarCraft II folder could not be read.",
  too_many_entries: "The folder exceeds the bank scan limit.",
  too_many_matches: "The folder exceeds the map association limit.",
  identifier_conflict: "The inventory could not distinguish two entries.",
};
const INVENTORY_FAILURE = "The bank inventory could not be loaded.";

type InventoryQuery =
  | "refresh_inventory"
  | "use_automatic_inventory"
  | "select_inventory_folder";

export function openInventoryBank(id: string, control: string) {
  return invoke<FileDialogReply>("open_bank_id", { id, control });
}

export function useInventory({
  onOpenBank,
  disabled,
  selectedBankId,
}: {
  onOpenBank: (id: string) => void;
  disabled: boolean;
  selectedBankId?: string;
}): MapInventory {
  const [snapshot, setSnapshot] = useState<InventorySnapshot>();
  const [query, setQuery] = useState<InventoryQuery>();
  const [message, setMessage] = useState<string>();
  const [cancelled, setCancelled] = useState(false);
  const [map, setMap] = useState<string | null>(null);
  const active = useRef(false);

  const run = useCallback(async (command: InventoryQuery) => {
    if (active.current) return;
    active.current = true;
    setMessage(undefined);
    setCancelled(false);
    setQuery(command);
    try {
      const result = await invoke<InventorySnapshot | null>(command);
      if (result) setSnapshot(result);
    } catch (error) {
      if (error === "cancelled") setCancelled(true);
      else
        setMessage(
          error === "busy"
            ? "A scan or native file dialog is already open."
            : INVENTORY_FAILURE,
        );
    } finally {
      active.current = false;
      setQuery(undefined);
    }
  }, []);

  useEffect(() => {
    void run("refresh_inventory");
  }, [run]);

  const controls = {
    onRefresh: () => void run("refresh_inventory"),
    onSelectDirectory: () => void run("select_inventory_folder"),
    onUseAutomatic: snapshot?.manual
      ? () => void run("use_automatic_inventory")
      : undefined,
    onCancel: () => {
      void invoke("cancel_inventory").catch(() =>
        setMessage("The scan could not be canceled."),
      );
    },
  };
  const rootName = snapshot?.rootName;
  if (query) return { status: "scanning", controls, rootName };
  if (cancelled) return { status: "cancelled", controls, rootName };
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
        disabled,
      },
      incomplete: snapshot.incomplete,
      message,
      controls,
      rootName,
    };
  }
  if (message || snapshot?.status === "failed") {
    return {
      status: "failed",
      message:
        message ??
        (snapshot?.reason && FAILURE_TEXT[snapshot.reason]) ??
        INVENTORY_FAILURE,
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
