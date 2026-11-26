import { Separator } from "@heroui/react/separator";
import { useId, useState } from "react";

export interface MapListItem {
  id: string;
  name: string;
  isBlizzard: boolean;
  hasBankFiles: boolean;
}

export interface MapFilters {
  hideBlizzardMaps: boolean;
  hideMapsWithoutBanks: boolean;
}

export type MapInventory =
  | { status: "unavailable" }
  | { status: "root-not-found" }
  | {
      status: "ready";
      maps: readonly MapListItem[];
      selectedMapId: string | null;
      onSelectMap: (mapId: string) => void;
    };

export interface MapOptions {
  autoResign?: { enabled: boolean; onChange: (enabled: boolean) => void };
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
  const [filters, setFilters] = useState(INITIAL_FILTERS);
  const maps =
    inventory.status === "ready"
      ? filterMaps(inventory.maps, query, filters)
      : [];
  const autoResign = options?.autoResign;
  const ready = inventory.status === "ready";

  function emptyMessage() {
    if (inventory.status === "root-not-found") {
      return "No SC2 Bank Folder path auto-detected, please select a folder";
    }
    if (inventory.status === "unavailable") return "No maps loaded.";
    return inventory.maps.length > 0
      ? "No maps match the current search and filters."
      : "No maps or bank files found.";
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
            onChange={(event) => setQuery(event.currentTarget.value)}
            placeholder="Search maps…"
            disabled={!ready}
          />
        </label>
      </header>

      <div className="map-list">
        {ready && maps.length > 0 && (
          <ul aria-label="Maps">
            {maps.map((map) => (
              <li key={map.id}>
                <button
                  type="button"
                  onClick={() => inventory.onSelectMap(map.id)}
                  aria-current={
                    inventory.selectedMapId === map.id ? "true" : undefined
                  }
                >
                  {map.name}
                </button>
              </li>
            ))}
          </ul>
        )}
        <p className="empty-message text-sm" role="status">
          {maps.length === 0 ? emptyMessage() : null}
        </p>
      </div>

      <div className="map-options">
        <Separator className="mb-2.5" isDecorative />
        <fieldset>
          <legend className="mb-1 text-sm font-semibold">Quick Options</legend>
          <label>
            <input
              type="checkbox"
              checked={filters.hideBlizzardMaps}
              disabled={!ready}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                setFilters((current) => ({
                  ...current,
                  hideBlizzardMaps: checked,
                }));
              }}
            />
            Hide Blizzard maps
          </label>
          <label>
            <input
              type="checkbox"
              checked={filters.hideMapsWithoutBanks}
              disabled={!ready}
              onChange={(event) => {
                const checked = event.currentTarget.checked;
                setFilters((current) => ({
                  ...current,
                  hideMapsWithoutBanks: checked,
                }));
              }}
            />
            Hide maps without bank files
          </label>
          {autoResign && (
            <label>
              <input
                type="checkbox"
                checked={autoResign.enabled}
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
