import { AppShell, type MapInventory } from "@starbank/ui";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  type BankListItem,
  bankScopeChoices,
  filterBanks,
} from "../../../packages/ui/src/inventory-browser";

const maps = [
  { id: "alpha", name: "Alpha", isBlizzard: false, hasBankFiles: true },
  { id: "beta", name: "Beta", isBlizzard: false, hasBankFiles: true },
  { id: "empty", name: "Empty", isBlizzard: false, hasBankFiles: false },
];

const banks: BankListItem[] = [
  { id: "first", name: "Shared.SC2Bank", mapIds: ["alpha"] },
  {
    id: "ambiguous",
    name: "Shared.SC2Bank",
    mapIds: ["alpha", "beta"],
    playerHandle: "synthetic-player",
    authorHandle: "synthetic-author",
  },
  { id: "unknown", name: "Unknown.SC2Bank", mapIds: [] },
];

function render(inventory: MapInventory) {
  return renderToStaticMarkup(
    <AppShell name="StarBank Reloaded" inventory={inventory} />,
  );
}

describe("desktop inventory projection", () => {
  it("preserves all candidate associations and isolates unmatched files", () => {
    expect(filterBanks(banks, { kind: "map", mapId: "alpha" })).toEqual([
      banks[0],
      banks[1],
    ]);
    expect(filterBanks(banks, { kind: "map", mapId: "beta" })).toEqual([
      banks[1],
    ]);
    expect(filterBanks(banks, { kind: "unmatched" })).toEqual([banks[2]]);
    expect(filterBanks(banks, { kind: "all" })).toBe(banks);
  });

  it("shows scoped ambiguity for a selected map and retains folder controls", () => {
    const markup = render({
      status: "ready",
      maps,
      selectedMapId: "beta",
      onSelectMap: () => {},
      rootName: "Fixture Banks",
      incomplete: true,
      controls: { onSelectDirectory: () => {}, onRefresh: () => {} },
      bankBrowser: { banks, selectedBankId: "ambiguous", onOpenBank: () => {} },
    });

    expect(markup).toContain("Ambiguous map match (2 candidates)");
    expect(markup).toContain("synthetic-player · synthetic-author");
    expect(markup).toContain("Some inventory results could not be loaded.");
    expect(markup).toContain("Fixture Banks");
    expect(markup).toContain("Select Folder…");
    expect(markup).toContain("Refresh");
    expect(markup).not.toContain("Unknown.SC2Bank");
  });

  it("offers unmatched files even when no cache maps were discovered", () => {
    const markup = render({
      status: "ready",
      maps: [],
      selectedMapId: null,
      onSelectMap: () => {},
      bankBrowser: { banks: [banks[2]], onOpenBank: () => {} },
    });

    expect(markup).toContain("No cached maps found.");
    expect(markup).toContain("Unknown.SC2Bank");
    expect(markup).toContain("Unmatched map");
  });

  it("offers scan cancellation and separates permission failures from no root", () => {
    const scanning = render({
      status: "scanning",
      scanned: 5,
      total: 20,
      controls: {
        onSelectDirectory: () => {},
        onRefresh: () => {},
        onCancel: () => {},
      },
    });

    expect(scanning).toContain("Cancel scan");
    expect(scanning).toContain("5 inspected");
    expect(scanning).toContain('max="20"');
    expect(scanning).not.toContain(">Refresh<");
    expect(
      render({ status: "failed", message: "The folder cannot be read." }),
    ).toContain("The folder cannot be read.");
    expect(render({ status: "cancelled" })).toContain("Scan canceled.");
    expect(render({ status: "manual-required" })).toContain(
      "Automatic SC2 folder detection is unavailable.",
    );
  });

  it("bounds the initially rendered bank rows", () => {
    const markup = render({
      status: "ready",
      maps: [],
      selectedMapId: null,
      onSelectMap: () => {},
      bankBrowser: {
        banks: Array.from({ length: 81 }, (_, index) => ({
          id: String(index),
          name: `Fixture ${index + 1}.SC2Bank`,
          mapIds: [],
        })),
        onOpenBank: () => {},
      },
    });

    expect(markup).toContain("Fixture 80.SC2Bank");
    expect(markup).not.toContain("Fixture 81.SC2Bank");
    expect(markup).toContain("Show more banks");
  });
});

describe("bank scopes and archive dates", () => {
  const scopedBanks: BankListItem[] = [
    {
      id: "copy-one",
      name: "Same.SC2Bank",
      mapIds: ["alpha"],
      scopeId: "one",
      playerHandle: "1-S2-1-42",
    },
    {
      id: "copy-two",
      name: "Same.SC2Bank",
      mapIds: [],
      scopeId: "two",
      playerHandle: "1-S2-1-42",
    },
    { id: "unknown", name: "Unknown.SC2Bank", mapIds: [] },
  ];

  it("keeps identically labeled account scopes and unscoped banks distinct", () => {
    const choices = bankScopeChoices(scopedBanks);
    expect(choices).toEqual([
      { key: "scope:one", label: "1-S2-1-42 (1)" },
      { key: "scope:two", label: "1-S2-1-42 (2)" },
    ]);
    expect(filterBanks(scopedBanks, { kind: "all" })).toBe(scopedBanks);
    expect(filterBanks(scopedBanks, { kind: "unmatched" })).toEqual(
      scopedBanks.slice(1),
    );
    expect(
      filterBanks(scopedBanks, { kind: "unmatched" }, "scope:two"),
    ).toEqual([scopedBanks[1]]);
    expect(
      filterBanks(scopedBanks, { kind: "map", mapId: "alpha" }, "scope:two"),
    ).toEqual([]);
    expect(
      filterBanks(scopedBanks, { kind: "map", mapId: "alpha" }, "scope:one"),
    ).toEqual([scopedBanks[0]]);
  });

  it("offers the account selector only with multiple known scopes", () => {
    function show(banks: readonly BankListItem[]) {
      return render({
        status: "ready",
        maps: [],
        selectedMapId: null,
        onSelectMap: () => {},
        bankBrowser: { banks, onOpenBank: () => {} },
      });
    }

    const multiple = show(scopedBanks);
    expect(multiple).toContain("Bank account or player");
    expect(multiple).toContain("All accounts");
    expect(multiple).toContain("1-S2-1-42 (1)");
    expect(multiple).toContain("1-S2-1-42 (2)");
    expect(show([scopedBanks[0], scopedBanks[2]])).not.toContain(
      "Bank account or player",
    );
  });

  it("uses known player handles only when an opaque scope is absent", () => {
    const legacy = [
      { id: "x", name: "X.SC2Bank", mapIds: [], playerHandle: "known-handle" },
      { id: "y", name: "Y.SC2Bank", mapIds: [], playerHandle: "known-handle" },
      {
        id: "z",
        name: "Z.SC2Bank",
        mapIds: [],
        playerHandle: "other-handle",
        scopeId: "z",
      },
    ];
    expect(bankScopeChoices(legacy)).toEqual([
      { key: "player:known-handle", label: "known-handle" },
      { key: "scope:z", label: "other-handle" },
    ]);
    expect(filterBanks(legacy, { kind: "all" }, "player:known-handle")).toEqual(
      legacy.slice(0, 2),
    );
  });

  it("renders supplied author and a valid archive modification time", () => {
    const millis = Date.UTC(2024, 1, 29, 12);
    const markup = render({
      status: "ready",
      maps: [
        { ...maps[0], author: "Fixture Studio", modifiedAtMillis: millis },
        { ...maps[1], modifiedAtMillis: Number.MAX_SAFE_INTEGER },
      ],
      selectedMapId: null,
      onSelectMap: () => {},
    });
    expect(markup).toContain("Fixture Studio");
    expect(markup).toContain(
      'Modified <time dateTime="2024-02-29T12:00:00.000Z"',
    );
    expect(markup.match(/<time /g)).toHaveLength(1);
    expect(markup).not.toContain("Downloaded");
  });
});
