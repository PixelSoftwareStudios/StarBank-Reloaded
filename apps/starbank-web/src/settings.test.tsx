import { renderToStaticMarkup } from "react-dom/server";
import { SettingsPage } from "starbank-ui/settings";
import {
  authorizeLegacyRoot,
  BROWSER_SETTINGS_KEY,
  decodeLegacySettings,
  decodeSettings,
  defaultSettings,
  readBrowserSettings,
  recoverBrowserSettings,
  type SettingsStorage,
  writeBrowserSettings,
} from "starbank-ui/settings-model";
import { describe, expect, test } from "vitest";

function mockStorage(initial?: string) {
  const data = new Map<string, string>();
  if (initial !== undefined) data.set(BROWSER_SETTINGS_KEY, initial);
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, next: string) => {
      data.set(key, next);
    },
  } satisfies SettingsStorage;
}

describe("browser preferences", () => {
  test("preserves the original quick defaults and reloads saved changes", () => {
    const storage = mockStorage();
    const defaults = defaultSettings();
    expect(readBrowserSettings(storage)).toEqual({
      kind: "missing",
      settings: defaults,
    });
    expect(defaults).toMatchObject({
      version: 1,
      hideBlizzardMaps: true,
      hideMapsWithoutBanks: true,
      autoResignOnSave: true,
      theme: "system",
      mapPaneWidth: 28,
    });

    const next = {
      ...defaults,
      hideBlizzardMaps: false,
      theme: "dark",
      mapPaneWidth: 31.5,
    } as const;
    expect(writeBrowserSettings(next, storage)).toEqual({ kind: "saved" });
    expect(readBrowserSettings(storage)).toEqual({
      kind: "loaded",
      settings: next,
    });
    expect(decodeSettings(storage.getItem(BROWSER_SETTINGS_KEY) ?? "")).toEqual(
      {
        kind: "loaded",
        settings: next,
      },
    );
  });

  test("migrates v1 settings without dropping existing settings or worker choices", () => {
    const defaults = defaultSettings();
    const { mapIndexWorkers: disappeared, ...legacy } = defaults;
    expect(disappeared).toBe("auto");
    const original = JSON.stringify({ ...legacy, theme: "dark" });
    const storage = mockStorage(original);
    expect(readBrowserSettings(storage)).toEqual({
      kind: "loaded",
      settings: { ...defaults, theme: "dark" },
    });
    expect(storage.getItem(BROWSER_SETTINGS_KEY)).toBe(original);

    for (const mapIndexWorkers of ["1", "2", "3", "4", "auto"] as const) {
      const next = { ...defaults, theme: "dark", mapIndexWorkers } as const;
      expect(writeBrowserSettings(next, storage)).toEqual({ kind: "saved" });
      expect(readBrowserSettings(storage)).toEqual({
        kind: "loaded",
        settings: next,
      });
    }
    for (const invalid of ["0", "5", 1, null]) {
      expect(
        decodeSettings(JSON.stringify({ ...legacy, mapIndexWorkers: invalid })),
      ).toEqual({
        kind: "error",
        issue: "invalid_settings",
      });
    }
  });

  test("retains corrupt settings until explicitly recovered", () => {
    const storage = mockStorage("{");
    const current = defaultSettings();
    expect(readBrowserSettings(storage)).toEqual({
      kind: "error",
      issue: "invalid_json",
    });
    expect(writeBrowserSettings(current, storage)).toEqual({
      kind: "error",
      issue: "invalid_json",
    });
    expect(storage.getItem(BROWSER_SETTINGS_KEY)).toBe("{");
    expect(recoverBrowserSettings(current, storage)).toEqual({ kind: "saved" });
    expect(storage.getItem(BROWSER_SETTINGS_KEY)).toBe(JSON.stringify(current));
    expect(recoverBrowserSettings(current, storage)).toEqual({
      kind: "error",
      issue: "settings_changed",
    });
  });

  test("keeps unknown settings versions intact even on explicit recovery", () => {
    const encoded = JSON.stringify({ ...defaultSettings(), version: 2 });
    const storage = mockStorage(encoded);
    expect(readBrowserSettings(storage)).toEqual({
      kind: "error",
      issue: "unsupported_version",
    });
    expect(recoverBrowserSettings(defaultSettings(), storage)).toEqual({
      kind: "error",
      issue: "unsupported_version",
    });
    expect(storage.getItem(BROWSER_SETTINGS_KEY)).toBe(encoded);
  });

  test("rejects malformed values and unexpected fields before committing", () => {
    const storage = mockStorage();
    const settings = { ...defaultSettings(), mapPaneWidth: 55 };
    expect(writeBrowserSettings(settings, storage)).toEqual({
      kind: "error",
      issue: "invalid_settings",
    });
    expect(
      writeBrowserSettings(
        { ...defaultSettings(), rawXml: "<Bank/>" } as ReturnType<
          typeof defaultSettings
        >,
        storage,
      ),
    ).toEqual({ kind: "error", issue: "invalid_settings" });
    expect(storage.getItem(BROWSER_SETTINGS_KEY)).toBeNull();
    expect(decodeSettings('{"version":1}')).toEqual({
      kind: "error",
      issue: "invalid_settings",
    });
    expect(decodeSettings(" ".repeat(64 * 1024 + 1))).toEqual({
      kind: "error",
      issue: "settings_too_large",
    });
  });

  test("reports blocked storage and failed writes", () => {
    const blocked = {
      getItem() {
        throw Error("private details");
      },
      setItem() {
        throw Error("unexpected write attempt");
      },
    };
    const diskFull = {
      getItem: () => null,
      setItem() {
        throw Error("private details");
      },
    };
    const unavailable = { kind: "error", issue: "storage_unavailable" };
    expect(readBrowserSettings(blocked)).toEqual(unavailable);
    expect(writeBrowserSettings(defaultSettings(), blocked)).toEqual(
      unavailable,
    );
    expect(writeBrowserSettings(defaultSettings(), diskFull)).toEqual(
      unavailable,
    );
  });
});

describe("legacy desktop settings", () => {
  test("preserves partial preferences and delegates path authorization", async () => {
    const sourcePath = "/synthetic/SC2 data";
    const migrated = decodeLegacySettings(
      JSON.stringify({
        hide_blizzard_maps: false,
        auto_resign_on_save: false,
        sc2_root_override: sourcePath,
        unrelated_legacy_field: null,
      }),
    );
    expect(migrated.kind).toBe("migrated");
    if (migrated.kind !== "migrated") return;
    expect(migrated.settings).toMatchObject({
      hideBlizzardMaps: false,
      hideMapsWithoutBanks: true,
      autoResignOnSave: false,
    });
    const seen: string[] = [];
    await expect(
      authorizeLegacyRoot(migrated, async (path) => {
        seen.push(path);
        return "root_123";
      }),
    ).resolves.toEqual({ kind: "manual", rootId: "root_123" });
    expect(seen).toEqual([sourcePath]);
    expect(JSON.stringify(migrated.settings)).not.toContain(sourcePath);
  });

  test("keeps denied and malformed root references pending", async () => {
    const migrated = decodeLegacySettings('{"sc2_root_override":"/synthetic"}');
    if (migrated.kind !== "migrated") throw Error("invalid fixture");
    for (const rejected of [null, "/not-an-id", "id\n", ""]) {
      await expect(
        authorizeLegacyRoot(migrated, async () => rejected),
      ).resolves.toEqual({ kind: "needs_authorization" });
    }
    const defaulted = decodeLegacySettings('{"auto_resign_on_save":null}');
    if (defaulted.kind !== "migrated") throw Error("invalid fixture");
    await expect(
      authorizeLegacyRoot(defaulted, async () => {
        throw Error("no path should be passed");
      }),
    ).resolves.toEqual({ kind: "automatic" });
    expect(defaulted.settings.autoResignOnSave).toBe(true);
    expect(decodeLegacySettings('{"hide_blizzard_maps":"false"}')).toEqual({
      kind: "error",
      issue: "invalid_settings",
    });
  });
});

test("settings render manual folder and backup controls only when available", () => {
  const noChange = () => undefined;
  const browser = renderToStaticMarkup(
    <SettingsPage
      settings={defaultSettings()}
      onBack={noChange}
      onChange={noChange}
    />,
  );
  expect(browser).toContain("Auto re-sign banks on save");
  expect(browser).not.toContain("Choose bank folder");
  expect(browser).not.toContain("Automatic backup");
  const desktop = renderToStaticMarkup(
    <SettingsPage
      settings={{ ...defaultSettings(), automaticBackup: "off" }}
      onBack={noChange}
      onChange={noChange}
      backupAvailable
      discovery={{
        root: { mode: "needs_authorization" },
        accounts: [{ id: "account_1", label: "Player One" }],
        accountId: "account_1",
        onChooseFolder: noChange,
        onUseAutomatic: noChange,
        onRescan: noChange,
        onSelectAccount: noChange,
      }}
    />,
  );
  expect(desktop).toContain("Choose bank folder");
  expect(desktop).toContain("Choose your previous bank folder again");
  expect(desktop).toContain("Player One");
  expect(desktop).toContain("without an automatic backup");
});

test("the map index worker selector is native-only and respects read-only preferences", () => {
  const noChange = () => undefined;
  const shared = {
    settings: { ...defaultSettings(), mapIndexWorkers: "2" },
    onChange: noChange,
    onBack: noChange,
    readOnly: true,
  } as const;
  const browser = renderToStaticMarkup(
    <SettingsPage
      {...shared}
      platformInfo={{ version: "0.1.0", platform: "web" }}
    />,
  );
  expect(browser).toContain("Map indexing is available in the desktop app.");
  expect(browser).not.toContain("Map index workers");
  const desktop = renderToStaticMarkup(
    <SettingsPage
      {...shared}
      platformInfo={{ version: "0.1.0", platform: "macos" }}
    />,
  );
  expect(desktop).toContain("Map index workers</span><select disabled=");
  expect(desktop).toContain(
    '<option value="2" selected="">At most 2 workers</option>',
  );
  expect(desktop).toContain("Applies to the next scan.");
});

test("settings offer map index rebuild alongside refresh when supported", () => {
  const noChange = () => undefined;
  const settings = renderToStaticMarkup(
    <SettingsPage
      settings={defaultSettings()}
      onBack={noChange}
      onChange={noChange}
      inventory={{
        status: "ready",
        maps: [],
        selectedMapId: null,
        onSelectMap: noChange,
        controls: { onRefresh: noChange, onRebuildMapIndex: noChange },
      }}
    />,
  );
  expect(settings).toContain('<button type="button">Refresh</button>');
  expect(settings).toContain("Rebuild map index</button>");
});

test("settings present archive progress with cancellation when preferences are read-only", () => {
  const noChange = () => undefined;
  const settings = renderToStaticMarkup(
    <SettingsPage
      settings={defaultSettings()}
      onBack={noChange}
      onChange={noChange}
      readOnly
      inventory={{
        status: "scanning",
        message: "Inspecting cached map metadata…",
        scanned: 2,
        total: 5,
        rootName: "Banks",
        controls: { onCancel: noChange, onRebuildMapIndex: noChange },
      }}
    />,
  );
  expect(settings).toContain("<legend>Bank discovery</legend>");
  expect(settings).toContain('aria-label="Bank discovery progress"');
  expect(settings).toContain('max="5" value="2"');
  expect(settings).toContain("2 of 5 map archives inspected");
  expect(settings).toContain('<button type="button">Cancel scan</button>');
  expect(settings).not.toContain("Rebuild map index</button>");
});

test.each([
  ["web", "Web browser", "Saving downloads a new bank copy."],
  ["macos", "macOS", "Saving requires local APFS."],
  ["linux", "Linux", "Overwrite is unavailable in this Linux build."],
  [
    "windows",
    "Windows",
    "Disk save and file export are unavailable in this Windows build.",
  ],
  [
    "desktop",
    "Desktop",
    "Disk save and file export are unavailable on this platform.",
  ],
] as const)(
  "settings describes the %s build without file paths",
  (platform, platformName, storage) => {
    const noChange = () => undefined;
    const markup = renderToStaticMarkup(
      <SettingsPage
        settings={defaultSettings()}
        onBack={noChange}
        onChange={noChange}
        readOnly
        platformInfo={{ version: "1.2.3", platform }}
      />,
    );
    expect(markup).toContain("<legend>Advanced</legend>");
    expect(markup).toContain("<dt>Version</dt><dd>1.2.3</dd>");
    expect(markup).toContain(`<dt>Platform</dt><dd>${platformName}</dd>`);
    expect(markup).toContain(storage);
  },
);
