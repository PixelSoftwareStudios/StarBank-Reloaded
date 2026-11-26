import * as v from "valibot";

export const SETTINGS_VERSION = 1;
export const MAP_PANE_WIDTH = { min: 20, max: 50, initial: 28 } as const;
export const BROWSER_SETTINGS_KEY = "starbank.preferences";
const MAX_SETTINGS_BYTES = 64 * 1024;
const MAX_NATIVE_PATH_LENGTH = 32767;
const MAX_ROOT_ID_LENGTH = 128;
export const MAX_BACKUP_RETENTION = 1_000;

const namedBackupFolder = v.pipe(v.string(), v.nonEmpty(), v.maxLength(4_096));
const backupPreferencesSchema = v.strictObject({
  location: v.variant("mode", [
    v.strictObject({ mode: v.literal("default") }),
    v.strictObject({ mode: v.literal("selected"), name: namedBackupFolder }),
    v.strictObject({
      mode: v.literal("needs_authorization"),
      name: namedBackupFolder,
    }),
  ]),
  keepNewest: v.nullable(
    v.pipe(
      v.number(),
      v.integer(),
      v.minValue(1),
      v.maxValue(MAX_BACKUP_RETENTION),
    ),
  ),
});

export type BackupPreferences = Readonly<
  v.InferOutput<typeof backupPreferencesSchema>
>;

export function decodeBackupPreferences(input: unknown): BackupPreferences {
  return v.parse(backupPreferencesSchema, input);
}

const settingsSchema = v.strictObject({
  version: v.literal(SETTINGS_VERSION),
  hideBlizzardMaps: v.boolean(),
  hideMapsWithoutBanks: v.boolean(),
  autoResignOnSave: v.boolean(),
  theme: v.picklist(["system", "light", "dark"]),
  density: v.picklist(["compact", "comfortable"]),
  textSize: v.picklist(["small", "normal", "large"]),
  mapPaneWidth: v.pipe(
    v.number(),
    v.minValue(MAP_PANE_WIDTH.min),
    v.maxValue(MAP_PANE_WIDTH.max),
  ),
  sectionsLayout: v.picklist(["separate", "unified"]),
  showBreadcrumbs: v.boolean(),
  wrapRaw: v.boolean(),
  groupNumbers: v.boolean(),
  automaticBackup: v.picklist(["first-overwrite", "every-overwrite", "off"]),
});

export type AppSettings = Readonly<v.InferOutput<typeof settingsSchema>>;

const defaults: AppSettings = {
  version: SETTINGS_VERSION,
  hideBlizzardMaps: true,
  hideMapsWithoutBanks: true,
  autoResignOnSave: true,
  theme: "system",
  density: "compact",
  textSize: "normal",
  mapPaneWidth: MAP_PANE_WIDTH.initial,
  sectionsLayout: "separate",
  showBreadcrumbs: true,
  wrapRaw: false,
  groupNumbers: true,
  automaticBackup: "first-overwrite",
};

export function defaultSettings(): AppSettings {
  return { ...defaults };
}

export type SettingsIssue =
  | "invalid_json"
  | "invalid_settings"
  | "unsupported_version"
  | "settings_too_large"
  | "storage_unavailable"
  | "settings_changed";

type SettingsFailure = {
  readonly kind: "error";
  readonly issue: SettingsIssue;
};

export type SettingsDecodeResult =
  | { readonly kind: "loaded"; readonly settings: AppSettings }
  | SettingsFailure;

export type SettingsLoadResult =
  | SettingsDecodeResult
  | { readonly kind: "missing"; readonly settings: AppSettings };

type ParsedJson =
  | { readonly kind: "parsed"; readonly value: unknown }
  | SettingsFailure;

function parseSettingsJson(raw: string): ParsedJson {
  if (
    raw.length > MAX_SETTINGS_BYTES ||
    new TextEncoder().encode(raw).byteLength > MAX_SETTINGS_BYTES
  ) {
    return { kind: "error", issue: "settings_too_large" };
  }
  try {
    const value: unknown = JSON.parse(raw);
    return { kind: "parsed", value };
  } catch {
    return { kind: "error", issue: "invalid_json" };
  }
}

export function decodeSettings(raw: string): SettingsDecodeResult {
  const input = parseSettingsJson(raw);
  if (input.kind === "error") return input;
  if (
    typeof input.value === "object" &&
    input.value !== null &&
    "version" in input.value &&
    Number.isInteger(input.value.version) &&
    input.value.version !== SETTINGS_VERSION
  ) {
    return { kind: "error", issue: "unsupported_version" };
  }
  const decoded = v.safeParse(settingsSchema, input.value);
  return decoded.success
    ? { kind: "loaded", settings: decoded.output }
    : { kind: "error", issue: "invalid_settings" };
}

const legacySchema = v.object({
  hide_blizzard_maps: v.nullish(v.boolean(), true),
  hide_maps_without_banks: v.nullish(v.boolean(), true),
  auto_resign_on_save: v.nullish(v.boolean(), true),
  sc2_root_override: v.nullish(
    v.pipe(
      v.string(),
      v.maxLength(MAX_NATIVE_PATH_LENGTH),
      v.check((path) => !path.includes("\0")),
    ),
    "",
  ),
});

export type LegacySettings = {
  readonly kind: "migrated";
  readonly settings: AppSettings;
  /** Keep this in the native importer until the selected path is authorized. */
  readonly rootPath?: string;
};

/** Decode only the original, unversioned Go preferences selected by the native importer. */
export function decodeLegacySettings(
  raw: string,
): LegacySettings | SettingsFailure {
  const input = parseSettingsJson(raw);
  if (input.kind === "error") return input;
  if (
    typeof input.value !== "object" ||
    input.value === null ||
    "version" in input.value
  ) {
    return { kind: "error", issue: "invalid_settings" };
  }
  const decoded = v.safeParse(legacySchema, input.value);
  if (!decoded.success) {
    return { kind: "error", issue: "invalid_settings" };
  }
  const legacy = decoded.output;
  return {
    kind: "migrated",
    settings: {
      ...defaultSettings(),
      hideBlizzardMaps: legacy.hide_blizzard_maps,
      hideMapsWithoutBanks: legacy.hide_maps_without_banks,
      autoResignOnSave: legacy.auto_resign_on_save,
    },
    ...(legacy.sc2_root_override ? { rootPath: legacy.sc2_root_override } : {}),
  };
}

/** A manual root reference issued by the native app after validating the path. */
export type NativeRootPreference =
  | { readonly kind: "automatic" }
  | { readonly kind: "manual"; readonly rootId: string }
  | { readonly kind: "needs_authorization" };

export async function authorizeLegacyRoot(
  legacy: LegacySettings,
  authorize: (path: string) => Promise<string | null>,
): Promise<NativeRootPreference> {
  if (legacy.rootPath === undefined) return { kind: "automatic" };
  try {
    const rootId = await authorize(legacy.rootPath);
    if (
      typeof rootId === "string" &&
      rootId.length > 0 &&
      rootId.length <= MAX_ROOT_ID_LENGTH &&
      !/[^A-Za-z0-9_-]/.test(rootId)
    ) {
      return { kind: "manual", rootId };
    }
  } catch {
    // The chosen legacy root remains pending if native path validation fails.
  }
  return { kind: "needs_authorization" };
}

export type SettingsStorage = Pick<Storage, "getItem" | "setItem">;

export function readBrowserSettings(
  storage?: SettingsStorage,
): SettingsLoadResult {
  try {
    const raw = (storage ?? window.localStorage).getItem(BROWSER_SETTINGS_KEY);
    return raw === null
      ? { kind: "missing", settings: defaultSettings() }
      : decodeSettings(raw);
  } catch {
    return { kind: "error", issue: "storage_unavailable" };
  }
}

export type SettingsWriteResult = { readonly kind: "saved" } | SettingsFailure;

/** Saves the shared preferences, leaving unreadable data available for explicit recovery. */
export function writeBrowserSettings(
  settings: AppSettings,
  storage?: SettingsStorage,
): SettingsWriteResult {
  const existing = readBrowserSettings(storage);
  if (existing.kind === "error") return existing;
  return persistBrowserSettings(settings, storage);
}

/** Replaces corrupt current-version settings after the user chooses to recover them. */
export function recoverBrowserSettings(
  settings: AppSettings,
  storage?: SettingsStorage,
): SettingsWriteResult {
  const existing = readBrowserSettings(storage);
  if (existing.kind !== "error") {
    return { kind: "error", issue: "settings_changed" };
  }
  if (
    existing.issue === "unsupported_version" ||
    existing.issue === "storage_unavailable"
  ) {
    return existing;
  }
  return persistBrowserSettings(settings, storage);
}

function persistBrowserSettings(
  settings: AppSettings,
  storage?: SettingsStorage,
): SettingsWriteResult {
  const checked = v.safeParse(settingsSchema, settings);
  if (!checked.success) return { kind: "error", issue: "invalid_settings" };
  try {
    (storage ?? window.localStorage).setItem(
      BROWSER_SETTINGS_KEY,
      JSON.stringify(checked.output),
    );
    return { kind: "saved" };
  } catch {
    return { kind: "error", issue: "storage_unavailable" };
  }
}
