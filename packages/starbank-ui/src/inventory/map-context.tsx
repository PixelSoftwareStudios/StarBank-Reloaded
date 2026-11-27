import {
  type BankBrowserModel,
  type BankListItem,
  filterBanks,
} from "./inventory-browser";
import type { MapInventory, MapListItem } from "./map-sidebar";

const ARCHIVE_DATE = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
});

export function MapArchiveDate({
  millis,
  className,
  showUnknown,
}: {
  millis?: number | null;
  className?: string;
  showUnknown?: boolean;
}) {
  const date =
    typeof millis === "number" && Number.isSafeInteger(millis)
      ? new Date(millis)
      : undefined;
  if (!date || Number.isNaN(date.getTime())) {
    return showUnknown ? (
      <span className={className}>Modified Unknown</span>
    ) : null;
  }
  const timestamp = date.toISOString();
  return (
    <span className={className}>
      Modified{" "}
      <time dateTime={timestamp} title={timestamp}>
        {ARCHIVE_DATE.format(date)}
      </time>
    </span>
  );
}

export interface MapContext {
  map: MapListItem;
  choices: readonly BankListItem[];
  activeBank?: BankListItem;
  onOpenBank?: BankBrowserModel["onOpenBank"];
  disabled?: boolean;
  uncertainMatch: boolean;
}

export function resolveMapContext(
  inventory: MapInventory,
  hasDocument: boolean,
  account: string,
): MapContext | undefined {
  if (inventory.status !== "ready") return undefined;
  const browser = inventory.bankBrowser;
  const activeBank = hasDocument
    ? browser?.banks.find(({ id }) => id === browser.selectedBankId)
    : undefined;

  // A sidebar selection does not change the provenance of the open bank.
  const selected = inventory.selectedMapId;
  const candidateIds = activeBank?.mapIds ?? [];
  let mapId = selected;
  if (hasDocument) {
    if (candidateIds.length === 1) mapId = candidateIds[0];
    else if (!selected || !candidateIds.includes(selected)) return undefined;
  }
  const map = inventory.maps.find(({ id }) => id === mapId);
  if (!map) return undefined;
  const choices = browser
    ? filterBanks(browser.banks, { kind: "map", mapId: map.id }, account)
    : [];
  return {
    map,
    activeBank,
    choices:
      activeBank && !choices.includes(activeBank)
        ? [activeBank, ...choices]
        : choices,
    onOpenBank: browser?.onOpenBank,
    disabled: browser?.disabled,
    uncertainMatch: candidateIds.length > 1,
  };
}

function bankOptionLabels(choices: readonly BankListItem[]) {
  const labels = choices.map(
    ({ name, displayPath, playerHandle, authorHandle }) =>
      [
        name,
        displayPath || [playerHandle, authorHandle].filter(Boolean).join(" · "),
      ]
        .filter(Boolean)
        .join(" — "),
  );
  const counts = new Map<string, number>();
  for (const label of labels) counts.set(label, (counts.get(label) ?? 0) + 1);
  const indexes = new Map<string, number>();
  return labels.map((label) => {
    if ((counts.get(label) ?? 0) <= 1) return label;
    const index = (indexes.get(label) ?? 0) + 1;
    indexes.set(label, index);
    return `${label} (${index})`;
  });
}

export function MapContextDetails({
  context,
  disabled,
  showBankName,
}: {
  context: MapContext;
  disabled?: boolean;
  showBankName?: boolean;
}) {
  const { map, choices, activeBank, onOpenBank } = context;
  const multiple = choices.length > 1;
  const chooseBank =
    onOpenBank && choices.length > 0 && (multiple || !activeBank);
  const labels = chooseBank ? bankOptionLabels(choices) : [];
  return (
    <>
      <div className="flex flex-wrap justify-between gap-x-3 gap-y-[0.175rem] wrap-anywhere text-[0.75rem] leading-[1.4] text-muted">
        <span>Created by {map.author || "Unknown"}</span>
        <MapArchiveDate millis={map.modifiedAtMillis} showUnknown />
      </div>
      {context.uncertainMatch && (
        <span className="wrap-anywhere text-[0.75rem] text-muted">
          Possible map match
        </span>
      )}
      {!chooseBank && showBankName && activeBank && (
        <span className="wrap-anywhere text-[0.75rem] text-muted">
          {activeBank.name}
        </span>
      )}
      {chooseBank && (
        <label className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[0.75rem]">
          {multiple && <span>Multiple bank files detected. Choose one:</span>}
          <select
            className="max-is-full rounded-[0.3rem] border border-line bg-field px-[0.35rem] py-[0.2rem] text-ink"
            aria-label="Bank file for this map"
            value={activeBank?.id ?? ""}
            disabled={disabled || context.disabled}
            onChange={(event) => {
              const id = event.currentTarget.value;
              if (id) onOpenBank(id);
            }}
          >
            {!activeBank && <option value="">Choose a bank file</option>}
            {choices.map((bank, index) => (
              <option key={bank.id} value={bank.id}>
                {labels[index]}
              </option>
            ))}
          </select>
        </label>
      )}
    </>
  );
}
