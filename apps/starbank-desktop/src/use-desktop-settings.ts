import {
  type AppSettings,
  decodeSettings,
  defaultSettings,
  type SettingsIssue,
} from "@starbank/platform/settings";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";

type PreferenceKey = Exclude<keyof AppSettings, "version">;

interface SettingsSnapshot {
  settings: AppSettings;
  legacyRootPending: boolean;
}

const ISSUE_MESSAGES: Record<SettingsIssue, string> = {
  invalid_json: "The saved settings could not be read.",
  invalid_settings: "The saved settings contain invalid preferences.",
  unsupported_version: "The saved settings use a different version.",
  settings_too_large: "The saved settings exceed the supported size.",
  storage_unavailable: "Desktop settings storage is unavailable.",
  settings_changed: "The saved settings changed. Review them and try again.",
};

function settingsIssue(error: unknown): SettingsIssue {
  return typeof error === "string" && Object.hasOwn(ISSUE_MESSAGES, error)
    ? (error as SettingsIssue)
    : "storage_unavailable";
}

function canRecover(issue: SettingsIssue | undefined): boolean {
  return issue === "invalid_json" || issue === "invalid_settings";
}

function decodeSnapshot(response: unknown): SettingsSnapshot {
  if (
    typeof response !== "object" ||
    response === null ||
    !("settings" in response) ||
    !("legacyRootPending" in response) ||
    typeof response.legacyRootPending !== "boolean"
  ) {
    throw new Error("Invalid native settings response.");
  }
  const decoded = decodeSettings(JSON.stringify(response.settings));
  if (decoded.kind === "error")
    throw new Error("Invalid native settings response.");
  return {
    settings: decoded.settings,
    legacyRootPending: response.legacyRootPending,
  };
}

async function readSettings(): Promise<SettingsSnapshot> {
  return decodeSnapshot(await invoke<unknown>("read_settings"));
}

/** Presents only confirmed native preferences and serializes rapid edits in UI order. */
export function useDesktopSettings() {
  const [snapshot, setSnapshot] = useState<SettingsSnapshot>();
  const [readIssue, setReadIssue] = useState<SettingsIssue>();
  const [writeIssue, setWriteIssue] = useState<SettingsIssue>();
  const generation = useRef(0);
  const pending = useRef(Promise.resolve());
  const recovering = useRef(false);

  useEffect(() => {
    const current = ++generation.current;
    void readSettings().then(
      (loaded) => {
        if (generation.current === current) setSnapshot(loaded);
      },
      (error: unknown) => {
        if (generation.current === current) setReadIssue(settingsIssue(error));
      },
    );
    return () => {
      generation.current += 1;
    };
  }, []);

  function enqueue(write: () => Promise<unknown>) {
    const current = generation.current;
    pending.current = pending.current.then(async () => {
      if (generation.current !== current) return;
      try {
        const saved = decodeSnapshot(await write());
        if (generation.current !== current) return;
        setSnapshot(saved);
        setReadIssue(undefined);
        setWriteIssue(undefined);
      } catch (error) {
        if (generation.current !== current) return;
        setWriteIssue(settingsIssue(error));
        try {
          const latest = await readSettings();
          if (generation.current !== current) return;
          setSnapshot(latest);
          setReadIssue(undefined);
        } catch (readError) {
          if (generation.current !== current) return;
          setSnapshot(undefined);
          setReadIssue(settingsIssue(readError));
        }
      }
    });
    return pending.current;
  }

  function change<Key extends PreferenceKey>(
    key: Key,
    value: AppSettings[Key],
  ) {
    if (!snapshot) return;
    void enqueue(() => invoke("change_setting", { change: { key, value } }));
  }

  function recover() {
    if (recovering.current) return;
    recovering.current = true;
    void enqueue(() => invoke("recover_settings")).finally(() => {
      recovering.current = false;
    });
  }

  const issue = writeIssue ?? readIssue;
  return {
    settings: snapshot?.settings ?? defaultSettings(),
    change,
    readOnly: snapshot === undefined,
    issue: issue === undefined ? undefined : ISSUE_MESSAGES[issue],
    recover: canRecover(readIssue) ? recover : undefined,
    legacyRootPending: snapshot?.legacyRootPending ?? false,
  };
}
