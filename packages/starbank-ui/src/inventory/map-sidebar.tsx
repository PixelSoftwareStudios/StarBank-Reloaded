import { Search } from "lucide-react";
import { useId, useMemo, useRef, useState } from "react";
import {
  SEARCH_SHORTCUT,
  useMapSearchShortcut,
} from "../editor/keyboard-shortcuts";
import { focusOutlineClasses } from "../shared/control-styles";
import {
  type AccountScopeSelection,
  BankBrowser,
  type BankBrowserModel,
  type BankFilter,
  InventoryActions,
  type InventoryControls,
  inventoryMetadataClasses,
  inventoryStatusClasses,
  mapsWithUnbackedBanks,
} from "./inventory-browser";
import { MapArchiveDate } from "./map-context";

const MAP_PAGE_SIZE = 80;
const mapListClasses =
  "min-h-0 max-h-[32dvh] overflow-auto overscroll-contain min-[42rem]:max-h-none";
const mapRowClasses = `${focusOutlineClasses} w-full rounded-control border border-transparent px-2 py-[0.32rem] text-start wrap-anywhere text-[0.875rem] leading-[1.35]`;
const mapOptionLabelClasses =
  "flex items-start gap-2 py-0.5 text-[0.8125rem] leading-[1.35] has-[input:disabled]:text-muted [.starbank-shell[data-density=comfortable]_&]:py-[0.35rem]";
const mapOptionInputClasses = `${focusOutlineClasses} size-3.5 shrink-0 accent-focus [margin-block-start:0.2rem]`;
const mapDividerClasses = "border-0 bg-line [block-size:1px]";

export interface MapListItem {
  id: string;
  name: string;
  isBlizzard: boolean;
  hasBankFiles: boolean;
  author?: string | null;
  modifiedAtMillis?: number | null;
}

export interface MapFilters {
  hideBlizzardMaps: boolean;
  hideMapsWithoutBanks: boolean;
}

type InventoryStatus =
  | {
      status: "unavailable" | "root-not-found" | "manual-required";
    }
  | { status: "cancelled"; message?: string }
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
  accountScope,
}: {
  id: string;
  inventory: MapInventory;
  options?: MapOptions;
  accountScope: AccountScopeSelection;
}) {
  const titleId = useId();
  const search = useRef<HTMLInputElement>(null);
  useMapSearchShortcut(search);
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
  const bankList = browser?.banks;
  const backupStatuses = browser?.backupStatuses;
  const account = accountScope.selected;
  const mapsWithMissingBackups = useMemo(
    () => mapsWithUnbackedBanks(bankList, backupStatuses, account),
    [bankList, backupStatuses, account],
  );
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
        return inventory.message ?? "Scan canceled.";
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
    <aside
      className="map-pane grid min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] gap-2.5 border-t border-line bg-surface p-3 [grid-area:maps] min-[42rem]:min-h-0 min-[42rem]:border-e min-[42rem]:border-t-0"
      id={id}
      aria-labelledby={titleId}
    >
      <header>
        <h2 className="text-sm font-semibold" id={titleId}>
          Map List
        </h2>
        <hr className={`my-2 ${mapDividerClasses}`} />
        <label className="relative block">
          <span className="sr-only">Search maps</span>
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute start-2.5 size-[0.9375rem] text-muted [inset-block-start:50%] [translate:0_-50%]"
          />
          <input
            className={`${focusOutlineClasses} w-full rounded-control border border-control-edge bg-field px-2 py-1 ps-[1.9rem] text-sm text-inherit [min-block-size:var(--control-height)] placeholder:text-muted`}
            ref={search}
            aria-keyshortcuts={ready ? SEARCH_SHORTCUT : undefined}
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

      <div
        className={
          browser
            ? "grid min-h-0 gap-2 overflow-auto min-[42rem]:grid-rows-[minmax(0,3fr)_minmax(min(6.75rem,52%),2fr)]"
            : "min-h-0 overflow-auto"
        }
      >
        <div
          className={
            browser ? mapListClasses : `${mapListClasses} min-[42rem]:h-full`
          }
        >
          {ready && (inventory.message || inventory.incomplete) && (
            <p className={inventoryStatusClasses} role="status">
              {inventory.message}
              {inventory.message && inventory.incomplete && " "}
              {inventory.incomplete &&
                "Some inventory results could not be loaded."}
            </p>
          )}
          <p className="text-sm text-muted empty:hidden" role="status">
            {statusMessage()}
          </p>
          {scanning && (
            <div className={`grid gap-1 ${inventoryStatusClasses}`}>
              <progress
                className="w-full accent-focus"
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
                    className={`${mapRowClasses} aria-[current=true]:border-s-[color:var(--selection-edge)] aria-[current=true]:bg-selected aria-[current=true]:font-semibold [&:hover:where(:not([aria-current=true]))]:bg-hover`}
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
                      <span className={inventoryMetadataClasses}>
                        {map.author}
                      </span>
                    )}
                    <MapArchiveDate
                      millis={map.modifiedAtMillis}
                      className={inventoryMetadataClasses}
                    />
                    <MapBackupStatus
                      count={mapsWithMissingBackups.get(map.id)}
                    />
                  </button>
                </li>
              ))}
            </ul>
          )}
          {maps.length > limit && (
            <button
              className={`${mapRowClasses} bg-field text-ink [&:hover]:border-focus [&:hover]:bg-hover`}
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
            accountScope={accountScope}
            onFilterChange={setBankScope}
          />
        )}
      </div>

      <div className="map-options">
        <hr className={`mb-2 ${mapDividerClasses}`} />
        <fieldset>
          <legend className="mb-1 text-sm font-semibold">Quick Options</legend>
          <label className={mapOptionLabelClasses}>
            <input
              className={mapOptionInputClasses}
              type="checkbox"
              checked={filters.hideBlizzardMaps}
              disabled={!ready || options?.filters?.disabled}
              onChange={(event) =>
                changeFilter("hideBlizzardMaps", event.currentTarget.checked)
              }
            />
            Hide Blizzard maps
          </label>
          <label className={mapOptionLabelClasses}>
            <input
              className={mapOptionInputClasses}
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
            <label className={mapOptionLabelClasses}>
              <input
                className={mapOptionInputClasses}
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
            className={`${focusOutlineClasses} ms-auto block px-1 text-[0.8125rem] font-semibold text-focus [min-block-size:1.5rem] [&:hover]:underline`}
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

function MapBackupStatus({ count }: { count?: number }) {
  if (!count) return null;
  return (
    <span className={inventoryMetadataClasses}>
      {count === 1
        ? "1 bank without a recorded backup"
        : `${count} banks without recorded backups`}
    </span>
  );
}
