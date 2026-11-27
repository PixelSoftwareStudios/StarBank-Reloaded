import {
  type CSSProperties,
  lazy,
  type ReactNode,
  Suspense,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import type { BankEditorController } from "starbank-editor";
import { type AppSettings, MAP_PANE_WIDTH } from "starbank-platform/settings";
import { EditorPane } from "../editor/editor-pane";
import {
  type BankListItem,
  useBankAccountScope,
} from "../inventory/inventory-browser";
import { resolveMapContext } from "../inventory/map-context";
import {
  type MapInventory,
  type MapOptions,
  MapSidebar,
} from "../inventory/map-sidebar";
import type { ChangeSetting } from "../settings/settings-page";
import type { SettingsPlatformInfo } from "../settings/settings-platform";
import { SidebarSplitter } from "./sidebar-splitter";

interface PreferencesController {
  settings: AppSettings;
  change: ChangeSetting;
  issue?: string;
  recover?: () => void;
  readOnly?: boolean;
  automaticBackupAvailable?: boolean;
}

let settingsPage:
  | Promise<typeof import("../settings/settings-page")>
  | undefined;

export function preloadSettingsPage() {
  settingsPage ??= import("../settings/settings-page");
  return settingsPage;
}

const SettingsPage = lazy(async () => ({
  default: (await preloadSettingsPage()).SettingsPage,
}));

const NO_INVENTORY: MapInventory = { status: "unavailable" };
const NO_BANKS: readonly BankListItem[] = [];

export function AppShell({
  name,
  inventory = NO_INVENTORY,
  mapOptions,
  editor,
  preferences,
  workspaceAccessory,
  tools,
  breadcrumbAvailable,
  platformInfo,
  backupSettings,
}: {
  name: string;
  inventory?: MapInventory;
  mapOptions?: MapOptions;
  editor?: BankEditorController;
  preferences?: PreferencesController;
  workspaceAccessory?: ReactNode;
  tools?: ReactNode;
  breadcrumbAvailable?: boolean;
  platformInfo?: SettingsPlatformInfo;
  backupSettings?: ReactNode;
}) {
  const mapPaneId = useId();
  const accountScope = useBankAccountScope(
    inventory.status === "ready"
      ? (inventory.bankBrowser?.banks ?? NO_BANKS)
      : undefined,
  );
  const [mapPaneWidth, setMapPaneWidth] = useState<number>(
    MAP_PANE_WIDTH.initial,
  );
  const [previewWidth, setPreviewWidth] = useState<number>();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const previousFocus = useRef<HTMLElement | null>(null);
  const settings = preferences?.settings;
  const shownWidth = previewWidth ?? settings?.mapPaneWidth ?? mapPaneWidth;
  const showSettings = settingsOpen && preferences !== undefined;

  useEffect(() => {
    if (!showSettings && previousFocus.current) {
      previousFocus.current.focus();
      previousFocus.current = null;
    }
  }, [showSettings]);

  function openSettings() {
    const focused = document.activeElement;
    previousFocus.current = focused instanceof HTMLElement ? focused : null;
    setSettingsOpen(true);
  }

  const onOpenSettings = preferences
    ? openSettings
    : mapOptions?.onOpenSettings;
  const effectiveOptions: MapOptions | undefined = preferences
    ? {
        ...mapOptions,
        autoResign: {
          enabled: preferences.settings.autoResignOnSave,
          onChange: (value) => preferences.change("autoResignOnSave", value),
          disabled: preferences.readOnly,
        },
        filters: {
          values: preferences.settings,
          onChange: (key, value) => preferences.change(key, value),
          disabled: preferences.readOnly,
        },
        onOpenSettings,
      }
    : mapOptions;

  const style = { "--map-pane-width": `${shownWidth}%` } as CSSProperties;

  return (
    <div
      className="starbank-shell"
      style={style}
      data-density={settings?.density}
    >
      <main className="bank-workspace min-w-0">
        {workspaceAccessory}
        <div className="editor-surface" hidden={showSettings}>
          {editor ? (
            <EditorPane
              name={name}
              controller={editor}
              settings={settings}
              onOpenSettings={onOpenSettings}
              tools={tools}
              mapContext={resolveMapContext(
                inventory,
                Boolean(editor.document),
              )}
            />
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
        </div>
        {showSettings && (
          <div className="settings-surface">
            <Suspense fallback={<p role="status">Loading settings…</p>}>
              <SettingsPage
                settings={preferences.settings}
                inventory={inventory}
                accountScope={accountScope}
                onChange={preferences.change}
                onBack={() => setSettingsOpen(false)}
                issue={preferences.issue}
                onRecover={preferences.recover}
                readOnly={preferences.readOnly}
                backupAvailable={preferences.automaticBackupAvailable}
                breadcrumbAvailable={breadcrumbAvailable}
                revertToSavedAvailable={Boolean(editor?.onRevertToSaved)}
                focusOnMount
                platformInfo={platformInfo}
                backupSettings={backupSettings}
              />
            </Suspense>
          </div>
        )}
      </main>

      <SidebarSplitter
        disabled={preferences?.readOnly}
        controls={mapPaneId}
        percent={shownWidth}
        onResize={(width, commit) => {
          if (!preferences) {
            setMapPaneWidth(width);
          } else if (commit) {
            preferences.change("mapPaneWidth", width);
            setPreviewWidth(undefined);
          } else {
            setPreviewWidth(width);
          }
        }}
      />
      <MapSidebar
        id={mapPaneId}
        inventory={inventory}
        options={effectiveOptions}
        accountScope={accountScope}
      />
    </div>
  );
}
