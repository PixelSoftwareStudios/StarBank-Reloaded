import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useState } from "react";
import {
  type BackupPreferences,
  decodeBackupPreferences,
  MAX_BACKUP_RETENTION,
} from "starbank-ui/settings-model";

const ISSUE_MESSAGES = {
  unavailable: "Managed backup settings are unavailable.",
  unsafe_folder: "Choose an existing local folder supported by StarBank.",
  invalid_retention: `Enter a whole number from 1 to ${MAX_BACKUP_RETENTION}.`,
  record_invalid: "The saved backup preferences could not be read.",
  record_changed: "The backup preferences changed. Review them and try again.",
  folder_limit: "The saved managed backup folder list is full.",
  dialog_busy: "Finish the open file operation first.",
};

function message(error: unknown) {
  return typeof error === "string" && Object.hasOwn(ISSUE_MESSAGES, error)
    ? ISSUE_MESSAGES[error as keyof typeof ISSUE_MESSAGES]
    : ISSUE_MESSAGES.unavailable;
}

async function read() {
  return decodeBackupPreferences(await invoke("read_backup_preferences"));
}

export function useBackupPreferences(enabled: boolean) {
  const [preferences, setPreferences] = useState<BackupPreferences>();
  const [issue, setIssue] = useState<string>();

  useEffect(() => {
    if (!enabled) return;
    let active = true;
    void read().then(
      (value) => {
        if (active) setPreferences(value);
      },
      (error: unknown) => {
        if (active) setIssue(message(error));
      },
    );
    return () => {
      active = false;
    };
  }, [enabled]);

  async function update(
    command:
      | "choose_backup_location"
      | "use_default_backup_location"
      | "set_backup_retention",
    args?: { keepNewest: number | null },
  ) {
    setIssue(undefined);
    try {
      const response = await invoke(command, args);
      if (response === null) return false;
      setPreferences(decodeBackupPreferences(response));
      return true;
    } catch (error) {
      setIssue(message(error));
      try {
        setPreferences(await read());
      } catch {
        setPreferences(undefined);
      }
      return false;
    }
  }

  return { preferences, issue, update };
}

export function BackupPreferencesFields({
  preferences,
  issue,
  disabled,
  choose,
  useDefault,
  setKeepNewest,
}: {
  preferences?: BackupPreferences;
  issue?: string;
  disabled: boolean;
  choose: () => void;
  useDefault: () => void;
  setKeepNewest: (value: number | null) => void;
}) {
  const messageId = useId();
  return (
    <>
      <div className="settings-page-choice">
        <span>Managed backup folder</span>
        {preferences && (
          <strong className="min-w-0 break-words">
            {preferences.location.mode === "default"
              ? "App default"
              : preferences.location.name}
          </strong>
        )}
      </div>
      {preferences?.location.mode === "needs_authorization" && (
        <p className="settings-page-note" role="status">
          Choose this folder again or use the app default. Writes that require a
          managed backup are paused.
        </p>
      )}
      <div className="settings-page-actions">
        <button
          type="button"
          onClick={choose}
          disabled={disabled || !preferences}
        >
          Choose backup folder…
        </button>
        {preferences?.location.mode !== "default" && preferences && (
          <button type="button" onClick={useDefault} disabled={disabled}>
            Use app default
          </button>
        )}
      </div>
      {preferences ? (
        <BackupRetentionField
          key={preferences.keepNewest ?? "all"}
          keepNewest={preferences.keepNewest}
          setKeepNewest={setKeepNewest}
          messageId={messageId}
          disabled={disabled}
        />
      ) : !issue ? (
        <p role="status">Loading managed backup settings…</p>
      ) : null}
      {issue && <p role="alert">{issue}</p>}
    </>
  );
}

function BackupRetentionField({
  keepNewest,
  setKeepNewest,
  messageId,
  disabled,
}: {
  keepNewest: number | null;
  setKeepNewest: (value: number | null) => void;
  messageId: string;
  disabled: boolean;
}) {
  const [draft, setDraft] = useState(keepNewest?.toString() ?? "");
  const value = draft === "" ? null : Number(draft);
  const valid =
    value === null ||
    (/^\d+$/.test(draft) &&
      Number.isSafeInteger(value) &&
      value >= 1 &&
      value <= MAX_BACKUP_RETENTION);
  return (
    <>
      <div className="settings-page-actions">
        <label className="settings-page-choice">
          Newest managed backups per bank
          <input
            type="text"
            className="settings-retention-input"
            inputMode="numeric"
            size={8}
            value={draft}
            placeholder="Keep all"
            aria-describedby={messageId}
            aria-invalid={!valid}
            disabled={disabled}
            onChange={(event) => setDraft(event.currentTarget.value)}
          />
        </label>
        <button
          type="button"
          disabled={disabled || !valid || value === keepNewest}
          onClick={() => setKeepNewest(value)}
        >
          Save retention
        </button>
      </div>
      <p id={messageId} className="settings-page-note">
        {valid
          ? "Leave blank to keep all backups. The retention setting identifies older backups for review."
          : ISSUE_MESSAGES.invalid_retention}
      </p>
    </>
  );
}
