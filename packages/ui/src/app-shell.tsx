import { type CSSProperties, useId, useState } from "react";
import type { BankEditorController } from "./editor-model";
import { EditorPane } from "./editor-pane";
import { type MapInventory, type MapOptions, MapSidebar } from "./map-sidebar";
import { DEFAULT_MAP_PANE_WIDTH, SidebarSplitter } from "./sidebar-splitter";

const NO_INVENTORY: MapInventory = { status: "unavailable" };

export function AppShell({
  name,
  inventory = NO_INVENTORY,
  mapOptions,
  editor,
}: {
  name: string;
  inventory?: MapInventory;
  mapOptions?: MapOptions;
  editor?: BankEditorController;
}) {
  const mapPaneId = useId();
  const [mapPaneWidth, setMapPaneWidth] = useState(DEFAULT_MAP_PANE_WIDTH);
  const style = { "--map-pane-width": `${mapPaneWidth}%` } as CSSProperties;

  return (
    <div className="starbank-shell" style={style}>
      <main className="bank-workspace min-w-0">
        {editor ? (
          <EditorPane name={name} controller={editor} />
        ) : (
          <div className="editor-welcome">
            <h1 className="text-[1.75rem] leading-tight font-semibold">
              {name}
            </h1>
            <p className="empty-message">
              Bank editing is unavailable in this build.
            </p>
          </div>
        )}
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

export { preloadRawEditor } from "./bank-editor";
export {
  type BankEditorController,
  type BankEditorDocument,
  type EditorAction,
  type EditorActionResult,
  type EditorRow,
  type EditorSection,
  type EditorTarget,
  type EditorValueEdit,
  normalizeDraft,
  prepareTableDraft,
} from "./editor-model";
export {
  filterMaps,
  type MapFilters,
  type MapInventory,
  type MapListItem,
  type MapOptions,
} from "./map-sidebar";
