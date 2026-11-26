import { Dropdown } from "@heroui/react/dropdown";
import { type DragEvent, useRef } from "react";
import type {
  BankEditorController,
  EditorAction,
  EditorActionResult,
  EditorTarget,
} from "./editor-model";

export type RunEditorAction = (
  action: () => Promise<EditorActionResult>,
  onSuccess?: () => void,
) => void;

export function fileDropHandlers(
  onOpenFile: BankEditorController["onOpenFile"],
  disabled: boolean,
  run: RunEditorAction,
) {
  if (!onOpenFile) return {};
  return {
    onDragOver(event: DragEvent<HTMLDivElement>) {
      if (!event.dataTransfer.types.includes("Files")) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = disabled ? "none" : "copy";
    },
    onDrop(event: DragEvent<HTMLDivElement>) {
      const files = event.dataTransfer.files;
      if (!files.length) return;
      event.preventDefault();
      if (disabled) return;
      const file = files[0];
      if (files.length === 1 && file) run(() => onOpenFile(file));
      else
        run(async () => ({
          ok: false,
          message: "Open one bank file at a time.",
        }));
    },
  };
}

export function OpenBankButton({
  controller,
  disabled,
  run,
}: {
  controller: BankEditorController;
  disabled: boolean;
  run: RunEditorAction;
}) {
  const fileInput = useRef<HTMLInputElement>(null);
  const { onOpen, onOpenFile } = controller;
  if (!onOpen && !onOpenFile) return null;

  return (
    <>
      {onOpenFile && !onOpen && (
        <input
          ref={fileInput}
          type="file"
          accept=".sc2bank,.xml"
          aria-label="Bank file"
          hidden
          disabled={disabled}
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            if (file && !disabled) run(() => onOpenFile(file));
          }}
        />
      )}
      <button
        className="editor-button"
        title={
          controller.document?.dirty && onOpen
            ? "Save or reset the current bank before opening another"
            : "Open a local bank file"
        }
        type="button"
        disabled={disabled}
        onClick={() => {
          if (onOpen) run(onOpen);
          else fileInput.current?.click();
        }}
      >
        Open Bank…
      </button>
    </>
  );
}

interface Action {
  label: string;
  action: EditorAction | undefined;
}

export function EditorActions({
  controller,
  target,
  disabled,
  run,
}: {
  controller: BankEditorController;
  target: EditorTarget;
  disabled: boolean;
  run: RunEditorAction;
}) {
  const actions: Action[] = [
    { label: "Save", action: controller.onSave },
    { label: "Save As…", action: controller.onSaveAs },
    { label: "Export copy…", action: controller.onExport },
    { label: "Copy bank XML", action: controller.onCopy },
  ];
  const available = actions.flatMap(({ label, action }) =>
    action ? [{ label, action }] : [],
  );
  const [primary, ...secondary] = available;
  const { onReset, onUndo, onRedo } = controller;

  return (
    <div className="editor-file-actions">
      {onUndo && (
        <button
          className="editor-button"
          type="button"
          title="Undo last bank change"
          disabled={disabled}
          onClick={() => run(() => onUndo(target))}
        >
          Undo
        </button>
      )}
      {onRedo && (
        <button
          className="editor-button"
          type="button"
          title="Redo last reverted bank change"
          disabled={disabled}
          onClick={() => run(() => onRedo(target))}
        >
          Redo
        </button>
      )}
      {onReset && (
        <button
          className="editor-button editor-reset"
          type="button"
          title="Restore the originally opened bank values"
          disabled={disabled}
          onClick={() => {
            if (
              controller.document?.dirty &&
              !window.confirm("Restore the originally opened bank values?")
            )
              return;
            run(() => onReset(target));
          }}
        >
          Reset
        </button>
      )}
      {primary && (
        <div className="editor-save-split">
          <button
            className="editor-button editor-primary"
            type="button"
            title={primary.label}
            disabled={disabled}
            onClick={() => run(() => primary.action(target))}
          >
            {primary.label}
          </button>
          {secondary.length > 0 && (
            <Dropdown>
              <Dropdown.Trigger
                className="editor-button editor-primary editor-save-menu"
                aria-label="More save options"
                isDisabled={disabled}
              >
                <span aria-hidden="true">▾</span>
              </Dropdown.Trigger>
              <Dropdown.Popover
                placement="top end"
                className="editor-action-popover"
              >
                <Dropdown.Menu
                  aria-label="Save options"
                  onAction={(key) => {
                    const action = secondary.find(({ label }) => label === key);
                    if (action) run(() => action.action(target));
                  }}
                >
                  {secondary.map(({ label }) => (
                    <Dropdown.Item id={label} key={label}>
                      {label}
                    </Dropdown.Item>
                  ))}
                </Dropdown.Menu>
              </Dropdown.Popover>
            </Dropdown>
          )}
        </div>
      )}
    </div>
  );
}
