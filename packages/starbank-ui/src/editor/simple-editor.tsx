import { useId, useRef, useState } from "react";
import type { RunEditorAction } from "./editor-actions";
import type {
  EditorProfileController,
  EditorProfileDraft,
  EditorProfileField,
  EditorProfileInput,
  EditorTarget,
  MatchedEditorProfile,
} from "./index";

export function ProfilePicker({
  controller,
  target,
  disabled,
  run,
}: {
  controller: EditorProfileController;
  target: EditorTarget;
  disabled: boolean;
  run: RunEditorAction;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  const [author, setAuthor] = useState("");
  const canPick =
    !disabled && (!controller.requireAuthor || author.trim() !== "");

  return (
    <details className="profile-picker">
      <summary>Bank profile</summary>
      <div className="profile-picker-body">
        {controller.requireAuthor && (
          <label>
            Map author handle supplied by you
            <input
              type="text"
              value={author}
              disabled={disabled}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setAuthor(event.currentTarget.value)}
            />
          </label>
        )}
        <div className="profile-picker-actions">
          <input
            ref={fileInput}
            type="file"
            accept=".json,.yaml,.yml"
            aria-label="Bank profile file"
            hidden
            disabled={!canPick}
            onChange={(event) => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = "";
              if (!file || !canPick) return;
              const claimedAuthor = controller.requireAuthor
                ? author.trim()
                : undefined;
              run(() => controller.onLoad(target, file, claimedAuthor));
            }}
          />
          <button
            className="editor-button"
            type="button"
            disabled={!canPick}
            onClick={() => fileInput.current?.click()}
          >
            Load Profile…
          </button>
          {controller.current.status !== "unloaded" && (
            <button
              className="editor-button"
              disabled={disabled}
              type="button"
              onClick={() => run(() => controller.onClear(target))}
            >
              Clear profile
            </button>
          )}
        </div>
      </div>
    </details>
  );
}

export function SimpleEditor({
  profile,
  draft,
  disabled,
  onChange,
}: {
  profile: MatchedEditorProfile;
  draft?: EditorProfileDraft;
  disabled: boolean;
  onChange: (field: EditorProfileField, input: EditorProfileInput) => void;
}) {
  const inputPrefix = useId();
  return (
    <div className="simple-fields">
      {profile.groups.map((group) => (
        <fieldset key={group.fields[0]?.id}>
          <legend>{group.label}</legend>
          {group.description && (
            <p className="simple-description">{group.description}</p>
          )}
          {group.fields.map((field) => {
            const input =
              draft?.field_id === field.id ? draft.input : undefined;
            const blocked =
              disabled ||
              field.read_only ||
              (draft !== undefined && draft.field_id !== field.id);
            const inputId = `${inputPrefix}-${field.id}`;
            return (
              <label key={field.id} htmlFor={inputId} className="simple-field">
                <span>
                  {field.label}
                  {field.description && (
                    <small className="simple-description">
                      {field.description}
                    </small>
                  )}
                </span>
                {field.kind === "checkbox" ? (
                  <input
                    id={inputId}
                    type="checkbox"
                    data-profile-draft={input !== undefined || undefined}
                    checked={
                      input?.kind === "checked"
                        ? input.value
                        : field.checked === true
                    }
                    disabled={blocked}
                    onChange={(event) =>
                      onChange(field, {
                        kind: "checked",
                        value: event.currentTarget.checked,
                      })
                    }
                  />
                ) : (
                  <input
                    id={inputId}
                    type="text"
                    data-profile-draft={input !== undefined || undefined}
                    inputMode={field.kind === "number" ? "decimal" : "text"}
                    value={input?.kind === "text" ? input.value : field.value}
                    readOnly={blocked}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(event) =>
                      onChange(field, {
                        kind: "text",
                        value: event.currentTarget.value,
                      })
                    }
                  />
                )}
              </label>
            );
          })}
        </fieldset>
      ))}
      {draft?.preview && (
        <section
          className="simple-preview"
          aria-label="Proposed profile changes"
        >
          {draft.preview.changes.length > 0 && (
            <table>
              <caption>Proposed profile changes</caption>
              <thead>
                <tr>
                  <th scope="col">Field</th>
                  <th scope="col">Current</th>
                  <th scope="col">Proposed</th>
                </tr>
              </thead>
              <tbody>
                {draft.preview.changes.map((change) => (
                  <tr key={change.id}>
                    <th scope="row">
                      {profile.groups
                        .flatMap((group) => group.fields)
                        .find((field) => field.id === change.field_id)?.label ??
                        change.field_id}
                    </th>
                    <td>{change.previous}</td>
                    <td>{change.value}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      )}
    </div>
  );
}
