import { type CSSProperties, useId, useState } from "react";
import { type MapInventory, type MapOptions, MapSidebar } from "./map-sidebar";
import { DEFAULT_MAP_PANE_WIDTH, SidebarSplitter } from "./sidebar-splitter";

const NO_INVENTORY: MapInventory = { status: "unavailable" };

export function AppShell({
  name,
  inventory = NO_INVENTORY,
  mapOptions,
}: {
  name: string;
  inventory?: MapInventory;
  mapOptions?: MapOptions;
}) {
  const mapPaneId = useId();
  const [mapPaneWidth, setMapPaneWidth] = useState(DEFAULT_MAP_PANE_WIDTH);
  const style = { "--map-pane-width": `${mapPaneWidth}%` } as CSSProperties;

  return (
    <div className="starbank-shell" style={style}>
      <main className="bank-workspace grid min-w-0 place-items-center px-6 py-8 text-center">
        <div>
          <h1 className="text-[1.75rem] leading-tight font-semibold">{name}</h1>
          <p className="empty-message mt-2">
            Bank editing is unavailable in this build.
          </p>
        </div>
      </main>

      <SidebarSplitter
        controls={mapPaneId}
        percent={mapPaneWidth}
        onResize={setMapPaneWidth}
      />
      <MapSidebar id={mapPaneId} inventory={inventory} options={mapOptions} />
    </div>
  );
}

export {
  filterMaps,
  type MapFilters,
  type MapInventory,
  type MapListItem,
  type MapOptions,
} from "./map-sidebar";
