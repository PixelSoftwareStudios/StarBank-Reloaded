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
import { EditorPane, type OpenRecovery } from "../editor/editor-pane";
import type { BankEditorController } from "../editor/index";
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
import { type AppSettings, MAP_PANE_WIDTH } from "../settings/settings";
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
  openRecovery,
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
  openRecovery?: OpenRecovery;
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
  const [previewWidth, setPreviewWidth] = useState<{ width: number }>();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const previousFocus = useRef<HTMLElement | null>(null);
  const settings = preferences?.settings;
  const shownWidth =
    previewWidth?.width ?? settings?.mapPaneWidth ?? mapPaneWidth;
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

  function resizeMapPane(width: number, commit: boolean) {
    if (!preferences) {
      setMapPaneWidth(width);
      return;
    }

    const preview = { width };
    setPreviewWidth(preview);
    if (!commit) return;

    function finish() {
      setPreviewWidth((current) => (current === preview ? undefined : current));
    }
    void Promise.resolve(preferences.change("mapPaneWidth", width)).then(
      finish,
      finish,
    );
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
      className="starbank-shell grid min-h-dvh grid-rows-[minmax(12rem,1fr)_auto] [grid-template-areas:'workspace'_'maps'] bg-canvas text-ink min-[42rem]:h-dvh min-[42rem]:min-h-88 min-[42rem]:grid-cols-[var(--map-pane-width)_minmax(0,1fr)] min-[42rem]:grid-rows-[1fr] min-[42rem]:[grid-template-areas:'maps_workspace']"
      style={style}
      data-density={settings?.density}
    >
      <main className="bank-workspace min-h-0 min-w-0 [grid-area:workspace] has-[>.open-requests]:grid has-[>.open-requests]:grid-rows-[auto_minmax(0,1fr)] min-[42rem]:flex min-[42rem]:flex-col">
        {workspaceAccessory}
        <div
          className={`editor-surface min-h-0 min-[42rem]:h-full min-[42rem]:flex-[1_1_0]${openRecovery && !showSettings ? " grid grid-rows-[auto_minmax(0,1fr)]" : ""}`}
          hidden={showSettings}
        >
          {editor ? (
            <EditorPane
              name={name}
              controller={editor}
              openRecovery={openRecovery}
              settings={settings}
              onOpenSettings={onOpenSettings}
              tools={tools}
              mapContext={resolveMapContext(
                inventory,
                Boolean(editor.document),
                accountScope.selected,
              )}
            />
          ) : (
            <div className="editor-welcome grid min-h-full place-content-center justify-items-center gap-3 px-6 py-8 text-center">
              <h1 className="text-[1.75rem] leading-tight font-semibold">
                {name}
              </h1>
              <p className="empty-message text-muted">
                Bank editing is unavailable in this build.
              </p>
            </div>
          )}
        </div>
        {showSettings && (
          <div className="settings-surface min-h-0 min-[42rem]:h-full min-[42rem]:flex-[1_1_0]">
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
        onResize={resizeMapPane}
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
