import { useId, useRef, useState } from "react";
import { buttonClasses } from "../shared/control-styles";
import type { RunEditorAction } from "./editor-actions";
import type {
  EditorProfileController,
  EditorProfileDraft,
  EditorProfileField,
  EditorProfileInput,
  EditorTarget,
  MatchedEditorProfile,
} from "./index";

const profileInputClasses =
  "min-w-0 rounded-control px-1.5 py-1 [border:1px_solid_var(--border)]";
const previewCellClasses = "p-1 text-start wrap-anywhere";

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
    <details className="profile-picker text-[0.8125rem]">
      <summary className="w-fit cursor-pointer">Bank profile</summary>
      <div className="profile-picker-body my-2 grid gap-1.5">
        {controller.requireAuthor && (
          <label className="grid gap-1">
            Map author handle supplied by you
            <input
              type="text"
              className={profileInputClasses}
              value={author}
              disabled={disabled}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setAuthor(event.currentTarget.value)}
            />
          </label>
        )}
        <div className="profile-picker-actions flex flex-wrap gap-1.5">
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
            className={buttonClasses()}
            type="button"
            disabled={!canPick}
            onClick={() => fileInput.current?.click()}
          >
            Load Profile…
          </button>
          {controller.current.status !== "unloaded" && (
            <button
              className={buttonClasses()}
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
    <div className="simple-fields h-full overflow-auto p-2">
      {profile.groups.map((group) => (
        <fieldset className="[&+fieldset]:mt-3" key={group.fields[0]?.id}>
          <legend className="text-start font-semibold">{group.label}</legend>
          {group.description && (
            <p className="simple-description block [color:var(--muted-foreground)]">
              {group.description}
            </p>
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
              <label
                key={field.id}
                htmlFor={inputId}
                className="simple-field grid grid-cols-2 items-center gap-2 py-1"
              >
                <span>
                  {field.label}
                  {field.description && (
                    <small className="simple-description block [color:var(--muted-foreground)]">
                      {field.description}
                    </small>
                  )}
                </span>
                {field.kind === "checkbox" ? (
                  <input
                    id={inputId}
                    type="checkbox"
                    className="justify-self-start"
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
                    className={`${profileInputClasses} read-only:[color:var(--muted-foreground)]`}
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
          className="simple-preview mt-4"
          aria-label="Proposed profile changes"
        >
          {draft.preview.changes.length > 0 && (
            <table className="w-full">
              <caption className="text-start font-semibold">
                Proposed profile changes
              </caption>
              <thead>
                <tr>
                  <th className={previewCellClasses} scope="col">
                    Field
                  </th>
                  <th className={previewCellClasses} scope="col">
                    Current
                  </th>
                  <th className={previewCellClasses} scope="col">
                    Proposed
                  </th>
                </tr>
              </thead>
              <tbody>
                {draft.preview.changes.map((change) => (
                  <tr key={change.id}>
                    <th className={previewCellClasses} scope="row">
                      {profile.groups
                        .flatMap((group) => group.fields)
                        .find((field) => field.id === change.field_id)?.label ??
                        change.field_id}
                    </th>
                    <td className={previewCellClasses}>{change.previous}</td>
                    <td className={previewCellClasses}>{change.value}</td>
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
