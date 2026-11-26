import { useMemo, useState } from "react";

const BANK_PAGE_SIZE = 80;

export interface BankListItem {
  id: string;
  name: string;
  mapIds: readonly string[];
  playerHandle?: string | null;
  scopeId?: string | null;
  authorHandle?: string | null;
  displayPath?: string;
}

export interface BankBrowserModel {
  banks: readonly BankListItem[];
  selectedBankId?: string | null;
  activeBackupStatus?: string;
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
  onUseAutomatic?: () => void;
  onCancel?: () => void;
}

const ALL_ACCOUNTS = "";

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

export function InventoryActions({
  controls,
  scanning,
  rootName,
}: {
  controls: InventoryControls;
  scanning: boolean;
  rootName?: string | null;
}) {
  const { onSelectDirectory, onRefresh, onUseAutomatic, onCancel } = controls;
  return (
    <div className="inventory-actions">
      {rootName && (
        <p className="inventory-root" title={rootName}>
          <span className="sr-only">Bank folder </span>
          {rootName}
        </p>
      )}
      {onSelectDirectory && (
        <button type="button" disabled={scanning} onClick={onSelectDirectory}>
          Select Folder…
        </button>
      )}
      {onRefresh && !scanning && (
        <button type="button" onClick={onRefresh}>
          Refresh
        </button>
      )}
      {onUseAutomatic && !scanning && (
        <button type="button" onClick={onUseAutomatic}>
          Use auto-detect
        </button>
      )}
      {onCancel && scanning && (
        <button type="button" onClick={onCancel}>
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
    <span className="inventory-bank-scope" title={text}>
      {text}
    </span>
  );
}

function BankMatchLabel({ count }: { count: number }) {
  if (count === 1) return null;
  return (
    <span className="inventory-bank-match">
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
  selectedAccount,
  onAccountChange,
  onFilterChange,
}: {
  model: BankBrowserModel;
  filter: BankFilter;
  currentMapName?: string;
  selectedAccount: string;
  onAccountChange: (scope: string) => void;
  onFilterChange: (filter: "all" | "unmatched") => void;
}) {
  const heading = "Banks";
  const choices = useMemo(() => bankScopeChoices(model.banks), [model.banks]);
  const account =
    choices.length > 1 && choices.some(({ key }) => key === selectedAccount)
      ? selectedAccount
      : ALL_ACCOUNTS;
  return (
    <section className="inventory-banks" aria-label={heading}>
      <div className="inventory-bank-heading">
        <h3 className="text-sm font-semibold">{heading}</h3>
        {choices.length > 1 && (
          <label className="inventory-account">
            <span className="sr-only">Bank account or player</span>
            <select
              value={account}
              onChange={(event) => onAccountChange(event.currentTarget.value)}
            >
              <option value={ALL_ACCOUNTS}>All accounts</option>
              {choices.map(({ key, label }) => (
                <option key={key} value={key}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      <fieldset className="inventory-bank-filters">
        <legend className="sr-only">Bank file filters</legend>
        <button
          type="button"
          aria-pressed={filter.kind === "all"}
          onClick={() => onFilterChange("all")}
        >
          All banks
        </button>
        <button
          type="button"
          aria-pressed={filter.kind === "unmatched"}
          onClick={() => onFilterChange("unmatched")}
        >
          Unmatched
        </button>
      </fieldset>
      {filter.kind === "map" && currentMapName && (
        <p className="inventory-map-context" title={currentMapName}>
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
      <p className="empty-message text-sm" role="status">
        {message}
      </p>
    );
  }

  return (
    <div className="inventory-bank-list">
      <ul aria-label="Bank files">
        {banks.slice(0, limit).map((bank) => (
          <li key={bank.id}>
            <button
              type="button"
              aria-current={
                model.selectedBankId === bank.id ? "true" : undefined
              }
              disabled={model.disabled}
              onClick={() => model.onOpenBank(bank.id)}
            >
              <span className="inventory-bank-name">{bank.name}</span>
              <BankMatchLabel count={bank.mapIds.length} />
              <BankScopeLabel bank={bank} />
              {model.selectedBankId === bank.id && model.activeBackupStatus && (
                <span className="inventory-bank-backup">
                  {model.activeBackupStatus}
                </span>
              )}
            </button>
          </li>
        ))}
      </ul>
      {banks.length > limit && (
        <button
          className="inventory-more"
          type="button"
          onClick={() => setLimit(limit + BANK_PAGE_SIZE)}
        >
          Show more banks
        </button>
      )}
    </div>
  );
}
