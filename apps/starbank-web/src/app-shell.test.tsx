import { renderToStaticMarkup } from "react-dom/server";
import { AppShell, filterMaps, type MapListItem } from "starbank-ui";
import { expect, it } from "vitest";

const MAPS: MapListItem[] = [
  {
    id: "custom",
    name: "Custom Defense",
    isBlizzard: false,
    hasBankFiles: true,
  },
  {
    id: "official",
    name: "Official Defense",
    isBlizzard: true,
    hasBankFiles: true,
  },
  {
    id: "archive",
    name: "Archive Defense",
    isBlizzard: false,
    hasBankFiles: false,
  },
];

it("renders a truthful empty workspace before engine integration", () => {
  const markup = renderToStaticMarkup(<AppShell name="StarBank Web" />);

  expect(markup).toContain("StarBank Web");
  expect(markup).toContain("Map List");
  expect(markup).toContain("No maps loaded.");
  expect(markup).toContain("Bank editing is unavailable in this build.");
  expect(markup).toContain("Search maps…");
  expect(markup).not.toContain("<button");
  expect(markup).not.toContain("Auto re-sign banks on save");
});

it("shows the approved no-root message", () => {
  const markup = renderToStaticMarkup(
    <AppShell
      name="StarBank Reloaded"
      inventory={{ status: "root-not-found" }}
    />,
  );

  expect(markup).toContain(
    "No SC2 Bank Folder path auto-detected, please select a folder",
  );
});

it("applies both default filters while exposing only supplied settings", () => {
  const markup = renderToStaticMarkup(
    <AppShell
      name="StarBank Reloaded"
      inventory={{
        status: "ready",
        maps: MAPS,
        selectedMapId: "custom",
        onSelectMap: () => {},
      }}
      mapOptions={{
        autoResign: { enabled: true, onChange: () => {} },
        onOpenSettings: () => {},
      }}
    />,
  );

  expect(markup).toContain("Hide Blizzard maps");
  expect(markup).toContain("Hide maps without bank files");
  expect(markup).toContain("Auto re-sign banks on save");
  expect(markup).toContain("More…");
  expect(markup).toContain('aria-current="true">Custom Defense');
  expect(markup).not.toContain("Official Defense");
  expect(markup).not.toContain("Archive Defense");
});

it("combines case-insensitive search with independent visibility filters", () => {
  const filters = { hideBlizzardMaps: true, hideMapsWithoutBanks: false };

  expect(filterMaps(MAPS, "  dEfEnSe  ", filters).map((map) => map.id)).toEqual(
    ["custom", "archive"],
  );
  expect(filterMaps(MAPS, "official", filters)).toEqual([]);
  expect(
    filterMaps(MAPS, "official", { ...filters, hideBlizzardMaps: false }).map(
      (map) => map.id,
    ),
  ).toEqual(["official"]);
});
