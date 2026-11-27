import { settingsChoiceClasses } from "./settings-controls";

export type SettingsPlatform =
  | "web"
  | "macos"
  | "linux"
  | "windows"
  | "desktop";

export interface SettingsPlatformInfo {
  readonly version: string;
  readonly platform: SettingsPlatform;
  readonly canCreateBankCopy?: boolean;
}

const PLATFORM_DETAILS = {
  web: {
    name: "Web browser",
    storage:
      "Saving downloads a new bank copy. Open banks through the browser’s local file picker.",
  },
  macos: {
    name: "macOS",
    storage:
      "Saving requires local APFS. Overwrite also requires a regular bank file with one hard link and supported permissions and security metadata. Symlink paths are refused.",
  },
  linux: {
    name: "Linux",
    storage:
      "Save As requires an unused filename, a supported local filesystem, and a path without symlinks. Overwrite is unavailable in this Linux build.",
  },
  windows: {
    name: "Windows",
    storage:
      "Bank editing is available in memory. Disk save and file export are unavailable in this Windows build.",
  },
  desktop: {
    name: "Desktop",
    storage: "Disk save and file export are unavailable on this platform.",
  },
} satisfies Record<SettingsPlatform, { name: string; storage: string }>;

export function SettingsPlatformDetails({
  info,
}: {
  info: SettingsPlatformInfo;
}) {
  const details = PLATFORM_DETAILS[info.platform];
  const storage =
    info.platform === "windows" && info.canCreateBankCopy
      ? "Save As and file export require an unused filename on a local NTFS volume. Overwrite is unavailable in this Windows build."
      : details.storage;
  return (
    <>
      <dl>
        <div className={settingsChoiceClasses}>
          <dt>Version</dt>
          <dd>{info.version}</dd>
        </div>
        <div className={settingsChoiceClasses}>
          <dt>Platform</dt>
          <dd>{details.name}</dd>
        </div>
      </dl>
      <p className="wrap-anywhere text-[0.8125rem] text-muted">{storage}</p>
    </>
  );
}
