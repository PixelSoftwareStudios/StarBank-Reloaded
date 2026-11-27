import { useEffect, useState } from "react";
import {
  type AppSettings,
  BROWSER_SETTINGS_KEY,
  defaultSettings,
  readBrowserSettings,
  recoverBrowserSettings,
  type SettingsIssue,
  type SettingsLoadResult,
  writeBrowserSettings,
} from "starbank-ui/settings-model";

type PreferenceKey = Exclude<keyof AppSettings, "version">;

const ISSUE_MESSAGES: Record<SettingsIssue, string> = {
  invalid_json: "The saved settings could not be read.",
  invalid_settings: "The saved settings contain invalid preferences.",
  unsupported_version: "The saved settings use a different version.",
  settings_too_large: "The saved settings exceed the supported size.",
  storage_unavailable: "Browser settings storage is unavailable.",
  settings_changed:
    "The saved settings changed. Review them before recovering.",
};

function canRecover(issue: SettingsIssue): boolean {
  return (
    issue === "invalid_json" ||
    issue === "invalid_settings" ||
    issue === "settings_too_large"
  );
}

/** Owns web preferences without rewriting corrupt or future-version data on startup. */
export function useBrowserSettings() {
  const [loaded, setLoaded] = useState<SettingsLoadResult>(readBrowserSettings);
  const [writeIssue, setWriteIssue] = useState<SettingsIssue>();

  useEffect(() => {
    function refresh(event: StorageEvent) {
      if (event.key === BROWSER_SETTINGS_KEY || event.key === null) {
        setLoaded(readBrowserSettings());
        setWriteIssue(undefined);
      }
    }
    window.addEventListener("storage", refresh);
    return () => window.removeEventListener("storage", refresh);
  }, []);

  function change<Key extends PreferenceKey>(
    key: Key,
    value: AppSettings[Key],
  ) {
    const current = readBrowserSettings();
    if (current.kind === "error") {
      setLoaded(current);
      setWriteIssue(undefined);
      return;
    }

    const next: AppSettings = { ...current.settings, [key]: value };
    const result = writeBrowserSettings(next);
    if (result.kind === "error") {
      setWriteIssue(result.issue);
      return;
    }
    setLoaded({ kind: "loaded", settings: next });
    setWriteIssue(undefined);
  }

  function recover() {
    const initial = defaultSettings();
    const result = recoverBrowserSettings(initial);
    setLoaded(readBrowserSettings());
    setWriteIssue(result.kind === "error" ? result.issue : undefined);
  }

  const issue =
    writeIssue ?? (loaded.kind === "error" ? loaded.issue : undefined);

  return {
    settings: loaded.kind === "error" ? defaultSettings() : loaded.settings,
    change,
    readOnly: loaded.kind === "error",
    issue: issue === undefined ? undefined : ISSUE_MESSAGES[issue],
    recover:
      loaded.kind === "error" && canRecover(loaded.issue) ? recover : undefined,
  };
}
