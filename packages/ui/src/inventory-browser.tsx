import { useMemo, useState } from "react";

const BANK_PAGE_SIZE = 80;

export interface BankListItem {
  id: string;
  name: string;
  mapIds: readonly string[];
  playerHandle?: string | null;
  authorHandle?: string | null;
  displayPath?: string;
}

export interface BankBrowserModel {
  banks: readonly BankListItem[];
  selectedBankId?: string | null;
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

export function filterBanks(
  banks: readonly BankListItem[],
  filter: BankFilter,
) {
  switch (filter.kind) {
    case "all":
      return banks;
    case "unmatched":
      return banks.filter((bank) => bank.mapIds.length === 0);
    case "map":
      return banks.filter((bank) => bank.mapIds.includes(filter.mapId));
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
  onFilterChange,
}: {
  model: BankBrowserModel;
  filter: BankFilter;
  currentMapName?: string;
  onFilterChange: (filter: "all" | "unmatched") => void;
}) {
  const heading = "Banks";
  return (
    <section className="inventory-banks" aria-label={heading}>
      <h3 className="text-sm font-semibold">{heading}</h3>
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
        key={filter.kind === "map" ? `map:${filter.mapId}` : filter.kind}
        model={model}
        filter={filter}
      />
    </section>
  );
}

function BankList({
  model,
  filter,
}: {
  model: BankBrowserModel;
  filter: BankFilter;
}) {
  const [limit, setLimit] = useState(BANK_PAGE_SIZE);
  const banks = useMemo(
    () => filterBanks(model.banks, filter),
    [model.banks, filter],
  );
  if (banks.length === 0) {
    const message =
      filter.kind === "unmatched"
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
