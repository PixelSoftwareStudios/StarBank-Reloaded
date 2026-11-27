import { type ReactNode, useEffect, useRef } from "react";
import { type AppSettings, MAP_PANE_WIDTH } from "starbank-platform/settings";
import {
  AccountScopePicker,
  type AccountScopeSelection,
  InventoryActions,
} from "../inventory/inventory-browser";
import type { MapInventory } from "../inventory/map-sidebar";
import {
  SettingsPlatformDetails,
  type SettingsPlatformInfo,
} from "./settings-platform";

export type SettingsKey = Exclude<keyof AppSettings, "version">;
export type ChangeSetting = <Key extends SettingsKey>(
  key: Key,
  value: AppSettings[Key],
) => void;

interface AccountOption {
  readonly id: string;
  readonly label: string;
}

export interface SettingsDiscoveryControls {
  readonly root:
    | { readonly mode: "automatic" }
    | { readonly mode: "manual"; readonly label: string }
    | { readonly mode: "needs_authorization" };
  readonly accounts: readonly AccountOption[];
  readonly accountId: string | null;
  readonly onChooseFolder: () => void;
  readonly onUseAutomatic: () => void;
  readonly onRescan: () => void;
  readonly onSelectAccount: (id: string | null) => void;
}

export interface SettingsPageProps {
  readonly settings: AppSettings;
  readonly onChange: ChangeSetting;
  readonly onBack: () => void;
  readonly discovery?: SettingsDiscoveryControls;
  readonly inventory?: MapInventory;
  readonly accountScope?: AccountScopeSelection;
  readonly backupAvailable?: boolean;
  readonly issue?: string;
  readonly readOnly?: boolean;
  readonly focusOnMount?: boolean;
  readonly breadcrumbAvailable?: boolean;
  readonly revertToSavedAvailable?: boolean;
  readonly onRecover?: () => void;
  readonly platformInfo?: SettingsPlatformInfo;
  readonly backupSettings?: ReactNode;
}

type Choice<Value extends string> = readonly [value: Value, label: string];

const THEMES: readonly Choice<AppSettings["theme"]>[] = [
  ["system", "System"],
  ["light", "Light"],
  ["dark", "Dark"],
];

const DENSITIES: readonly Choice<AppSettings["density"]>[] = [
  ["compact", "Compact"],
  ["comfortable", "Comfortable"],
];

const TEXT_SIZES: readonly Choice<AppSettings["textSize"]>[] = [
  ["small", "Small"],
  ["normal", "Normal"],
  ["large", "Large"],
];

const SECTION_LAYOUTS: readonly Choice<AppSettings["sectionsLayout"]>[] = [
  ["separate", "Separate Sections pane"],
  ["unified", "All sections in one table"],
];

const BACKUP_POLICIES: readonly Choice<AppSettings["automaticBackup"]>[] = [
  ["first-overwrite", "Before the first overwrite"],
  ["every-overwrite", "Before every overwrite"],
  ["off", "Off"],
];

export function SettingsPage({
  settings,
  onChange,
  onBack,
  discovery,
  inventory,
  accountScope,
  backupAvailable = false,
  issue,
  readOnly = false,
  focusOnMount = false,
  breadcrumbAvailable = false,
  revertToSavedAvailable = false,
  onRecover,
  platformInfo,
  backupSettings,
}: SettingsPageProps) {
  const back = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (focusOnMount) back.current?.focus();
  }, [focusOnMount]);

  return (
    <section className="settings-page" aria-label="Settings">
      <header className="settings-page-header">
        <h1>Settings</h1>
        <button type="button" ref={back} onClick={onBack}>
          Back to editor
        </button>
      </header>

      {issue && (
        <div className="settings-page-issue" role="alert">
          <p>{issue}</p>
          {onRecover && (
            <button type="button" onClick={onRecover}>
              Recover settings
            </button>
          )}
        </div>
      )}

      {!discovery && inventory?.controls && (
        <Group title="Bank discovery">
          <InventoryDiscoveryFields inventory={inventory} />
          {accountScope && (
            <AccountScopePicker scope={accountScope} variant="settings" />
          )}
        </Group>
      )}

      <Group title="General" disabled={readOnly}>
        {discovery && <DiscoveryFields discovery={discovery} />}
        {!discovery && !inventory?.controls && (
          <p className="settings-page-note">
            Automatic bank discovery and account selection are available in the
            desktop app.
          </p>
        )}
        <Toggle
          checked={settings.hideBlizzardMaps}
          onChange={(value) => onChange("hideBlizzardMaps", value)}
        >
          Hide Blizzard maps
        </Toggle>
        <Toggle
          checked={settings.hideMapsWithoutBanks}
          onChange={(value) => onChange("hideMapsWithoutBanks", value)}
        >
          Hide maps without bank files
        </Toggle>
      </Group>

      <Group title="Appearance" disabled={readOnly}>
        <ChoiceField
          label="Theme"
          choices={THEMES}
          selected={settings.theme}
          onChange={(value) => onChange("theme", value)}
        />
        <ChoiceField
          label="Density"
          choices={DENSITIES}
          selected={settings.density}
          onChange={(value) => onChange("density", value)}
        />
        <ChoiceField
          label="Text size"
          choices={TEXT_SIZES}
          selected={settings.textSize}
          onChange={(value) => onChange("textSize", value)}
        />
      </Group>

      <Group title="Layout" disabled={readOnly}>
        <label className="settings-page-choice">
          <span>Map list width</span>
          <input
            type="range"
            min={MAP_PANE_WIDTH.min}
            max={MAP_PANE_WIDTH.max}
            step={0.1}
            value={settings.mapPaneWidth}
            aria-valuetext={`${settings.mapPaneWidth}%`}
            onChange={(event) =>
              onChange("mapPaneWidth", event.currentTarget.valueAsNumber)
            }
          />
        </label>
        <ChoiceField
          label="Table layout"
          choices={SECTION_LAYOUTS}
          selected={settings.sectionsLayout}
          onChange={(value) => onChange("sectionsLayout", value)}
        />
        {breadcrumbAvailable && (
          <Toggle
            checked={settings.showBreadcrumbs}
            onChange={(value) => onChange("showBreadcrumbs", value)}
          >
            Show path breadcrumbs
          </Toggle>
        )}
        <Toggle
          checked={settings.wrapRaw}
          onChange={(value) => onChange("wrapRaw", value)}
        >
          Wrap Raw View lines
        </Toggle>
      </Group>

      <Group title="Editing" disabled={readOnly}>
        <Toggle
          checked={settings.groupNumbers}
          onChange={(value) => onChange("groupNumbers", value)}
        >
          Group numbers with commas
        </Toggle>
      </Group>

      <Group title="Saving" disabled={readOnly}>
        <Toggle
          checked={settings.autoResignOnSave}
          onChange={(value) => onChange("autoResignOnSave", value)}
        >
          Auto re-sign banks on save
        </Toggle>
        <p className="settings-page-note">
          Reset restores the bank as it was opened.
          {revertToSavedAvailable &&
            " Use Revert to Last Save to restore the last verified save."}
        </p>
        {backupAvailable && (
          <>
            <ChoiceField
              label="Automatic backup"
              choices={BACKUP_POLICIES}
              selected={settings.automaticBackup}
              onChange={(value) => onChange("automaticBackup", value)}
            />
            {settings.automaticBackup === "off" && (
              <p className="settings-page-note">
                Overwriting a bank will proceed without an automatic backup.
              </p>
            )}
          </>
        )}
        {backupSettings}
      </Group>
      {platformInfo && (
        <Group title="Advanced">
          <SettingsPlatformDetails info={platformInfo} />
        </Group>
      )}
    </section>
  );
}

function Group({
  title,
  children,
  disabled,
}: {
  title: string;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <fieldset className="settings-page-group" disabled={disabled}>
      <legend>{title}</legend>
      {children}
    </fieldset>
  );
}

function Toggle({
  checked,
  onChange,
  children,
}: {
  checked: boolean;
  onChange: (enabled: boolean) => void;
  children: ReactNode;
}) {
  return (
    <label className="settings-page-toggle">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
      {children}
    </label>
  );
}

function ChoiceField<Value extends string>({
  label,
  choices,
  selected,
  onChange,
}: {
  label: string;
  choices: readonly Choice<Value>[];
  selected: Value;
  onChange: (value: Value) => void;
}) {
  return (
    <label className="settings-page-choice">
      <span>{label}</span>
      <select
        value={selected}
        onChange={(event) => {
          const current = event.currentTarget.value;
          const choice = choices.find(([value]) => value === current);
          if (choice) onChange(choice[0]);
        }}
      >
        {choices.map(([value, caption]) => (
          <option key={value} value={value}>
            {caption}
          </option>
        ))}
      </select>
    </label>
  );
}

function InventoryDiscoveryFields({ inventory }: { inventory: MapInventory }) {
  const { controls, rootName } = inventory;
  const scanning = inventory.status === "scanning";
  let status: string;
  switch (inventory.status) {
    case "scanning":
      status = inventory.message ?? "Scanning bank folders…";
      break;
    case "ready":
      status =
        inventory.message ??
        (inventory.incomplete
          ? "Some inventory results could not be loaded."
          : "Bank inventory ready.");
      break;
    case "cancelled":
      status = inventory.message ?? "Bank scan canceled.";
      break;
    case "failed":
      status = inventory.message;
      break;
    case "manual-required":
      status = "Automatic bank discovery is unavailable. Select a folder.";
      break;
    case "root-not-found":
      status = "No SC2 Bank Folder path auto-detected, please select a folder";
      break;
    case "unavailable":
      status = "Bank inventory has not been loaded.";
      break;
  }

  return (
    <>
      {controls && (
        <InventoryActions
          controls={controls}
          rootName={rootName}
          scanning={scanning}
          onRebuildMapIndex={controls.onRebuildMapIndex}
        />
      )}
      <p className="settings-page-note" role="status">
        {status}
      </p>
      {scanning && (
        <progress
          aria-label="Bank discovery progress"
          max={inventory.total || undefined}
          value={inventory.total ? inventory.scanned : undefined}
        />
      )}
      {scanning &&
        inventory.scanned !== undefined &&
        inventory.total !== undefined && (
          <p className="settings-page-note">
            {inventory.scanned} of {inventory.total} map archives inspected
          </p>
        )}
    </>
  );
}

function DiscoveryFields({
  discovery,
}: {
  discovery: SettingsDiscoveryControls;
}) {
  const root = discovery.root;
  return (
    <>
      <p className="settings-page-note">
        {root.mode === "manual"
          ? root.label
          : root.mode === "needs_authorization"
            ? "Choose your previous bank folder again to authorize access."
            : "Detect bank folders automatically."}
      </p>
      <div className="settings-page-actions">
        <button type="button" onClick={discovery.onChooseFolder}>
          Choose bank folder…
        </button>
        <button
          type="button"
          onClick={discovery.onUseAutomatic}
          disabled={root.mode === "automatic"}
        >
          Use auto-detection
        </button>
        <button type="button" onClick={discovery.onRescan}>
          Re-scan
        </button>
      </div>
      {discovery.accounts.length > 0 && (
        <label className="settings-page-choice">
          <span>Account</span>
          <select
            value={discovery.accountId ?? ""}
            onChange={(event) =>
              discovery.onSelectAccount(event.currentTarget.value || null)
            }
          >
            <option value="">All accounts</option>
            {discovery.accounts.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
      )}
    </>
  );
}
