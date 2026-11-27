import { useEffect, useMemo, useState } from "react";
import {
  settingsButtonClasses,
  settingsChoiceClasses,
  settingsControlClasses,
} from "../settings/settings-controls";
import { focusOutlineClasses } from "../shared/control-styles";

const BANK_PAGE_SIZE = 80;

export const inventoryMetadataClasses =
  "block text-[0.75rem] font-normal text-muted";
export const inventoryStatusClasses = "text-[0.8125rem] text-muted";

const truncatedStatusClasses = `min-w-0 truncate ${inventoryStatusClasses}`;
const actionClasses = `${focusOutlineClasses} rounded-control border border-control-edge bg-field px-[0.45rem] py-[0.2rem] text-[0.8125rem] text-ink`;
const inventoryRowClasses = `${focusOutlineClasses} block w-full rounded-control border border-transparent px-[0.45rem] py-[0.3rem] text-start text-[0.875rem]`;

export interface BankListItem {
  id: string;
  name: string;
  mapIds: readonly string[];
  playerHandle?: string | null;
  scopeId?: string | null;
  authorHandle?: string | null;
  displayPath?: string;
}

export type BankBackupPresence = "recorded" | "never_backed_up" | "unknown";

export interface BankBrowserModel {
  banks: readonly BankListItem[];
  selectedBankId?: string | null;
  activeBackupStatus?: string;
  backupStatuses?: ReadonlyMap<string, BankBackupPresence>;
  onOpenBank: (bankId: string) => void;
  disabled?: boolean;
}

export type BankFilter =
  | { kind: "all" }
  | { kind: "unmatched" }
  | { kind: "map"; mapId: string };

export interface InventoryControls {
  onSelectDirectory?: () => void;
  onRefresh?: () => void;
  onRebuildMapIndex?: () => void;
  onUseAutomatic?: () => void;
  onCancel?: () => void;
}

const ALL_ACCOUNTS = "";

interface BankScopeChoice {
  readonly key: string;
  readonly label: string;
}

export interface AccountScopeSelection {
  readonly choices: readonly BankScopeChoice[];
  readonly selected: string;
  readonly select: (key: string) => void;
}

export function resolveBankAccountScope(
  choices: readonly BankScopeChoice[],
  requested: string,
): string {
  return choices.length > 1 && choices.some(({ key }) => key === requested)
    ? requested
    : ALL_ACCOUNTS;
}

/** Keep scope during transient scans and forget scopes missing from the next ready inventory. */
export function useBankAccountScope(
  banks: readonly BankListItem[] | undefined,
): AccountScopeSelection {
  const [requested, setRequested] = useState(ALL_ACCOUNTS);
  const choices = useMemo(
    () => (banks ? bankScopeChoices(banks) : []),
    [banks],
  );
  const selected = resolveBankAccountScope(choices, requested);

  useEffect(() => {
    if (banks !== undefined && requested !== selected) setRequested(selected);
  }, [banks, requested, selected]);

  return {
    choices,
    selected,
    select: (key) => setRequested(resolveBankAccountScope(choices, key)),
  };
}

export function AccountScopePicker({
  scope,
  variant,
}: {
  scope: AccountScopeSelection;
  variant: "sidebar" | "settings";
}) {
  if (scope.choices.length <= 1) return null;
  const sidebar = variant === "sidebar";
  return (
    <label
      className={sidebar ? "w-[min(68%,9rem)] min-w-0" : settingsChoiceClasses}
    >
      <span className={sidebar ? "sr-only" : undefined}>
        {sidebar ? "Bank account or player" : "Account"}
      </span>
      <select
        className={
          sidebar
            ? `${focusOutlineClasses} w-full rounded-sm border border-line bg-field px-[0.3rem] py-[0.2rem] text-[0.8125rem] text-ink`
            : settingsControlClasses
        }
        value={scope.selected}
        onChange={(event) => scope.select(event.currentTarget.value)}
      >
        <option value={ALL_ACCOUNTS}>All accounts</option>
        {scope.choices.map(({ key, label }) => (
          <option key={key} value={key}>
            {label}
          </option>
        ))}
      </select>
    </label>
  );
}

function scopeKey(bank: BankListItem) {
  if (bank.scopeId) return `scope:${bank.scopeId}`;
  if (bank.playerHandle) return `player:${bank.playerHandle}`;
  return undefined;
}

export function bankScopeChoices(banks: readonly BankListItem[]) {
  const scopes = new Map<string, string>();
  for (const bank of banks) {
    const key = scopeKey(bank);
    if (key && !scopes.has(key))
      scopes.set(key, bank.playerHandle || "Account");
  }

  const counts = new Map<string, number>();
  for (const label of scopes.values()) {
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }

  const indexes = new Map<string, number>();
  return Array.from(scopes, ([key, label]) => {
    if ((counts.get(label) ?? 0) <= 1) return { key, label };
    const next = (indexes.get(label) ?? 0) + 1;
    indexes.set(label, next);
    return { key, label: `${label} (${next})` };
  });
}

export function filterBanks(
  banks: readonly BankListItem[],
  filter: BankFilter,
  scope = ALL_ACCOUNTS,
) {
  const inScope =
    scope === ALL_ACCOUNTS
      ? banks
      : banks.filter((bank) => scopeKey(bank) === scope);
  switch (filter.kind) {
    case "all":
      return inScope;
    case "unmatched":
      return inScope.filter((bank) => bank.mapIds.length === 0);
    case "map":
      return inScope.filter((bank) => bank.mapIds.includes(filter.mapId));
  }
}

export function mapsWithUnbackedBanks(
  banks: readonly BankListItem[] | undefined,
  statuses: ReadonlyMap<string, BankBackupPresence> | undefined,
  account: string,
): ReadonlyMap<string, number> {
  const counts = new Map<string, number>();
  if (!banks || !statuses) return counts;
  for (const bank of filterBanks(banks, { kind: "all" }, account)) {
    if (statuses.get(bank.id) !== "never_backed_up" || bank.mapIds.length !== 1)
      continue;
    const [mapId] = bank.mapIds;
    if (mapId !== undefined) counts.set(mapId, (counts.get(mapId) ?? 0) + 1);
  }
  return counts;
}

function bankBackupLabel(model: BankBrowserModel, bankId: string) {
  if (model.selectedBankId === bankId && model.activeBackupStatus)
    return model.activeBackupStatus;
  switch (model.backupStatuses?.get(bankId)) {
    case "never_backed_up":
      return "No backups recorded";
    case "recorded":
      return "Backup recorded";
    default:
      return undefined;
  }
}

export function InventoryActions({
  controls,
  scanning,
  rootName,
  onRebuildMapIndex,
  variant = "sidebar",
}: {
  controls: InventoryControls;
  scanning: boolean;
  rootName?: string | null;
  onRebuildMapIndex?: () => void;
  variant?: "sidebar" | "settings";
}) {
  const buttonClasses = `${variant === "settings" ? settingsButtonClasses : actionClasses} disabled:cursor-not-allowed [&:hover:where(:not(:disabled))]:border-focus disabled:text-muted`;
  const { onSelectDirectory, onRefresh, onUseAutomatic, onCancel } = controls;
  return (
    <div className="[margin-block-start:0.5rem] flex flex-wrap gap-[0.3rem]">
      {rootName && (
        <p
          className={`basis-full leading-[1.25] ${truncatedStatusClasses}`}
          title={rootName}
        >
          <span className="sr-only">Bank folder </span>
          {rootName}
        </p>
      )}
      {onSelectDirectory && (
        <button
          className={buttonClasses}
          type="button"
          disabled={scanning}
          onClick={onSelectDirectory}
        >
          Select Folder…
        </button>
      )}
      {onRefresh && !scanning && (
        <button className={buttonClasses} type="button" onClick={onRefresh}>
          Refresh
        </button>
      )}
      {onRebuildMapIndex && !scanning && (
        <button
          className={buttonClasses}
          type="button"
          onClick={onRebuildMapIndex}
          title="Re-read all cached map archives"
        >
          Rebuild map index
        </button>
      )}
      {onUseAutomatic && !scanning && (
        <button
          className={buttonClasses}
          type="button"
          onClick={onUseAutomatic}
        >
          Use auto-detect
        </button>
      )}
      {onCancel && scanning && (
        <button className={buttonClasses} type="button" onClick={onCancel}>
          Cancel scan
        </button>
      )}
    </div>
  );
}

function BankScopeLabel({ bank }: { bank: BankListItem }) {
  const scope = [bank.playerHandle, bank.authorHandle]
    .filter(Boolean)
    .join(" · ");
  const text = bank.displayPath || scope;
  if (!text) return null;
  return (
    <span className={`${inventoryMetadataClasses} wrap-anywhere`} title={text}>
      {text}
    </span>
  );
}

function BankMatchLabel({ count }: { count: number }) {
  if (count === 1) return null;
  return (
    <span className={inventoryMetadataClasses}>
      {count === 0
        ? "Unmatched map"
        : `Ambiguous map match (${count} candidates)`}
    </span>
  );
}

export function BankBrowser({
  model,
  filter,
  currentMapName,
  accountScope,
  onFilterChange,
}: {
  model: BankBrowserModel;
  filter: BankFilter;
  currentMapName?: string;
  accountScope: AccountScopeSelection;
  onFilterChange: (filter: "all" | "unmatched") => void;
}) {
  const heading = "Banks";
  const account = accountScope.selected;
  return (
    <section
      className="flex min-h-0 flex-col gap-[0.35rem] overflow-auto border-line [border-block-start-width:1px] [padding-block-start:0.5rem]"
      aria-label={heading}
    >
      <div className="flex shrink-0 items-center justify-between gap-[0.35rem]">
        <h3 className="text-sm font-semibold">{heading}</h3>
        <AccountScopePicker scope={accountScope} variant="sidebar" />
      </div>
      <fieldset className="flex shrink-0 flex-wrap gap-[0.3rem]">
        <legend className="sr-only">Bank file filters</legend>
        <button
          className={`${actionClasses} aria-pressed:bg-selected aria-pressed:font-semibold [&:hover]:border-focus`}
          type="button"
          aria-pressed={filter.kind === "all"}
          onClick={() => onFilterChange("all")}
        >
          All banks
        </button>
        <button
          className={`${actionClasses} aria-pressed:bg-selected aria-pressed:font-semibold [&:hover]:border-focus`}
          type="button"
          aria-pressed={filter.kind === "unmatched"}
          onClick={() => onFilterChange("unmatched")}
        >
          Unmatched
        </button>
      </fieldset>
      {filter.kind === "map" && currentMapName && (
        <p
          className={`shrink-0 ${truncatedStatusClasses}`}
          title={currentMapName}
        >
          {currentMapName}
        </p>
      )}
      <BankList
        key={JSON.stringify([filter, account])}
        model={model}
        filter={filter}
        account={account}
      />
    </section>
  );
}

function BankList({
  model,
  filter,
  account,
}: {
  model: BankBrowserModel;
  filter: BankFilter;
  account: string;
}) {
  const [limit, setLimit] = useState(BANK_PAGE_SIZE);
  const banks = useMemo(
    () => filterBanks(model.banks, filter, account),
    [model.banks, filter, account],
  );
  if (banks.length === 0) {
    const message =
      account !== ALL_ACCOUNTS
        ? "No banks match this account and filter."
        : filter.kind === "unmatched"
          ? "No unmatched banks found."
          : filter.kind === "map"
            ? "No banks associated with this map."
            : "No bank files found.";
    return (
      <p className="shrink-0 text-sm text-muted" role="status">
        {message}
      </p>
    );
  }

  return (
    <div className="min-h-0 max-h-[28dvh] overflow-auto overscroll-contain min-[42rem]:max-h-none">
      <ul aria-label="Bank files">
        {banks.slice(0, limit).map((bank) => {
          const backupLabel = bankBackupLabel(model, bank.id);
          return (
            <li key={bank.id}>
              <button
                className={`${inventoryRowClasses} disabled:cursor-not-allowed disabled:text-muted aria-[current=true]:border-s-[color:var(--selection-edge)] aria-[current=true]:bg-selected [&:hover:where(:not(:disabled,[aria-current=true]))]:bg-hover`}
                type="button"
                aria-current={
                  model.selectedBankId === bank.id ? "true" : undefined
                }
                disabled={model.disabled}
                onClick={() => model.onOpenBank(bank.id)}
              >
                <span className="block wrap-anywhere">{bank.name}</span>
                <BankMatchLabel count={bank.mapIds.length} />
                <BankScopeLabel bank={bank} />
                {backupLabel && (
                  <span className={inventoryMetadataClasses}>
                    {backupLabel}
                  </span>
                )}
              </button>
            </li>
          );
        })}
      </ul>
      {banks.length > limit && (
        <button
          className={`${inventoryRowClasses} bg-field text-ink [&:hover]:border-focus [&:hover]:bg-hover`}
          type="button"
          onClick={() => setLimit(limit + BANK_PAGE_SIZE)}
        >
          Show more banks
        </button>
      )}
    </div>
  );
}
