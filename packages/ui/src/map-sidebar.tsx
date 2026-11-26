import { Separator } from "@heroui/react/separator";
import { useId, useMemo, useState } from "react";
import {
  BankBrowser,
  type BankBrowserModel,
  type BankFilter,
  InventoryActions,
  type InventoryControls,
} from "./inventory-browser";

const MAP_PAGE_SIZE = 80;

export interface MapListItem {
  id: string;
  name: string;
  isBlizzard: boolean;
  hasBankFiles: boolean;
  author?: string | null;
}

export interface MapFilters {
  hideBlizzardMaps: boolean;
  hideMapsWithoutBanks: boolean;
}

type InventoryStatus =
  | {
      status:
        | "unavailable"
        | "root-not-found"
        | "manual-required"
        | "cancelled";
    }
  | { status: "failed"; message: string }
  | { status: "scanning"; message?: string; scanned?: number; total?: number }
  | {
      status: "ready";
      maps: readonly MapListItem[];
      selectedMapId: string | null;
      onSelectMap: (mapId: string) => void;
      bankBrowser?: BankBrowserModel;
      incomplete?: boolean;
      message?: string;
    };

export type MapInventory = InventoryStatus & {
  controls?: InventoryControls;
  rootName?: string | null;
};

export interface MapOptions {
  autoResign?: {
    enabled: boolean;
    disabled?: boolean;
    onChange: (enabled: boolean) => void;
  };
  filters?: {
    values: Readonly<MapFilters>;
    disabled?: boolean;
    onChange: (key: keyof MapFilters, enabled: boolean) => void;
  };
  onOpenSettings?: () => void;
}

const INITIAL_FILTERS: MapFilters = {
  hideBlizzardMaps: true,
  hideMapsWithoutBanks: true,
};

export function filterMaps(
  maps: readonly MapListItem[],
  query: string,
  filters: Readonly<MapFilters>,
): MapListItem[] {
  const search = query.trim().toLowerCase();
  return maps.filter(
    (map) =>
      (!filters.hideBlizzardMaps || !map.isBlizzard) &&
      (!filters.hideMapsWithoutBanks || map.hasBankFiles) &&
      map.name.toLowerCase().includes(search),
  );
}

export function MapSidebar({
  id,
  inventory,
  options,
}: {
  id: string;
  inventory: MapInventory;
  options?: MapOptions;
}) {
  const titleId = useId();
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(MAP_PAGE_SIZE);
  const [bankScope, setBankScope] = useState<"selected" | "all" | "unmatched">(
    "selected",
  );
  const [localFilters, setLocalFilters] = useState(INITIAL_FILTERS);
  const filters = options?.filters?.values ?? localFilters;
  const ready = inventory.status === "ready";
  const scanning = inventory.status === "scanning";
  const availableMaps = ready ? inventory.maps : undefined;

  function changeFilter(key: keyof MapFilters, value: boolean) {
    if (options?.filters) options.filters.onChange(key, value);
    else setLocalFilters((current) => ({ ...current, [key]: value }));
    setLimit(MAP_PAGE_SIZE);
  }

  const maps = useMemo(
    () => (availableMaps ? filterMaps(availableMaps, query, filters) : []),
    [availableMaps, query, filters],
  );
  const autoResign = options?.autoResign;
  const browser = ready ? inventory.bankBrowser : undefined;
  const selectedMapId = ready ? inventory.selectedMapId : null;
  const bankFilter = useMemo<BankFilter>(
    () =>
      bankScope === "selected"
        ? selectedMapId
          ? { kind: "map", mapId: selectedMapId }
          : { kind: "all" }
        : { kind: bankScope },
    [bankScope, selectedMapId],
  );

  function statusMessage() {
    switch (inventory.status) {
      case "unavailable":
        return "No maps loaded.";
      case "root-not-found":
        return "No SC2 Bank Folder path auto-detected, please select a folder";
      case "manual-required":
        return "Automatic SC2 folder detection is unavailable. Select a folder to continue.";
      case "scanning":
        return inventory.message ?? "Scanning for banks…";
      case "cancelled":
        return "Scan canceled.";
      case "failed":
        return inventory.message;
      case "ready":
        if (maps.length > 0) return null;
        if (inventory.maps.length > 0)
          return "No maps match the current search and filters.";
        return inventory.bankBrowser
          ? "No cached maps found."
          : "No maps or bank files found.";
    }
  }

  return (
    <aside className="map-pane min-w-0 p-3.5" id={id} aria-labelledby={titleId}>
      <header>
        <h2 className="text-sm font-semibold" id={titleId}>
          Map List
        </h2>
        <Separator className="my-2.5" isDecorative />
        <label className="map-search">
          <span className="sr-only">Search maps</span>
          <input
            type="search"
            value={query}
            onChange={(event) => {
              setQuery(event.currentTarget.value);
              setLimit(MAP_PAGE_SIZE);
            }}
            placeholder="Search maps…"
            disabled={!ready}
          />
        </label>
        {inventory.controls && (
          <InventoryActions
            controls={inventory.controls}
            scanning={scanning}
            rootName={inventory.rootName}
          />
        )}
      </header>

      <div className={browser ? "inventory-map-bank" : "inventory-maps"}>
        <div className="map-list">
          {ready && (inventory.message || inventory.incomplete) && (
            <p className="inventory-notice" role="status">
              {inventory.message}
              {inventory.message && inventory.incomplete && " "}
              {inventory.incomplete &&
                "Some inventory results could not be loaded."}
            </p>
          )}
          <p className="empty-message text-sm" role="status">
            {statusMessage()}
          </p>
          {scanning && (
            <div className="inventory-progress">
              <progress
                aria-label="Inventory scan progress"
                max={inventory.total ?? undefined}
                value={
                  inventory.total && inventory.scanned !== undefined
                    ? inventory.scanned
                    : undefined
                }
              />
              {inventory.scanned !== undefined && (
                <p>{inventory.scanned} inspected</p>
              )}
            </div>
          )}
          {ready && maps.length > 0 && (
            <ul aria-label="Maps">
              {maps.slice(0, limit).map((map) => (
                <li key={map.id}>
                  <button
                    type="button"
                    onClick={() => {
                      setBankScope("selected");
                      inventory.onSelectMap(map.id);
                    }}
                    aria-current={
                      selectedMapId === map.id && bankScope === "selected"
                        ? "true"
                        : undefined
                    }
                  >
                    {map.name}
                    {map.author && (
                      <span className="inventory-map-author">{map.author}</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
          {maps.length > limit && (
            <button
              className="inventory-more"
              type="button"
              onClick={() => setLimit(limit + MAP_PAGE_SIZE)}
            >
              Show more maps
            </button>
          )}
        </div>
        {browser && (
          <BankBrowser
            model={browser}
            filter={bankFilter}
            currentMapName={
              maps.some(({ id }) => id === selectedMapId)
                ? undefined
                : availableMaps?.find(({ id }) => id === selectedMapId)?.name
            }
            onFilterChange={setBankScope}
          />
        )}
      </div>

      <div className="map-options">
        <Separator className="mb-2.5" isDecorative />
        <fieldset>
          <legend className="mb-1 text-sm font-semibold">Quick Options</legend>
          <label>
            <input
              type="checkbox"
              checked={filters.hideBlizzardMaps}
              disabled={!ready || options?.filters?.disabled}
              onChange={(event) =>
                changeFilter("hideBlizzardMaps", event.currentTarget.checked)
              }
            />
            Hide Blizzard maps
          </label>
          <label>
            <input
              type="checkbox"
              checked={filters.hideMapsWithoutBanks}
              disabled={!ready || options?.filters?.disabled}
              onChange={(event) =>
                changeFilter(
                  "hideMapsWithoutBanks",
                  event.currentTarget.checked,
                )
              }
            />
            Hide maps without bank files
          </label>
          {autoResign && (
            <label>
              <input
                type="checkbox"
                checked={autoResign.enabled}
                disabled={autoResign.disabled}
                onChange={(event) =>
                  autoResign.onChange(event.currentTarget.checked)
                }
              />
              Auto re-sign banks on save
            </label>
          )}
        </fieldset>
        {options?.onOpenSettings && (
          <button
            className="map-options-more"
            type="button"
            onClick={options.onOpenSettings}
          >
            More…
          </button>
        )}
      </div>
    </aside>
  );
}

export type {
  BankBrowserModel,
  BankListItem,
  InventoryControls,
} from "./inventory-browser";
