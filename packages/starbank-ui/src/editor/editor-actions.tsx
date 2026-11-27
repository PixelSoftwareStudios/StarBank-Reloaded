import { Dropdown } from "@heroui/react/dropdown";
import { Separator } from "@heroui/react/separator";
import { ChevronDown, Settings, Wrench } from "lucide-react";
import { type DragEvent, Fragment, type Ref, useRef } from "react";
import type {
  BankEditorController,
  EditorAction,
  EditorActionResult,
  EditorTarget,
} from "starbank-editor";
import { EditorHelpButton, EditorTooltip } from "../shared/editor-tooltip";
import {
  REDO_SHORTCUT,
  SAVE_SHORTCUT,
  UNDO_SHORTCUT,
  useEditorShortcuts,
} from "./keyboard-shortcuts";

export type RunEditorAction = (
  action: () => Promise<EditorActionResult>,
  onSuccess?: () => void,
) => void;

export function ToolsMenuTrigger({
  ref,
  descriptionId,
  disabled,
}: {
  ref?: Ref<HTMLButtonElement>;
  descriptionId?: string;
  disabled?: boolean;
}) {
  return (
    <EditorTooltip
      help="Open bank and map tools"
      placement="bottom"
      disabled={disabled}
    >
      <Dropdown.Trigger
        ref={ref}
        className="editor-button"
        aria-label="Tools"
        aria-describedby={descriptionId}
        isDisabled={disabled}
      >
        <Wrench aria-hidden="true" className="settings-icon" />
      </Dropdown.Trigger>
    </EditorTooltip>
  );
}

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
      <EditorHelpButton
        className="editor-button"
        help="Choose a local bank file"
        placement="bottom"
        disabled={disabled}
        onClick={() => {
          if (onOpen) run(onOpen);
          else fileInput.current?.click();
        }}
      >
        Open Bank…
      </EditorHelpButton>
    </>
  );
}

const menuGroups = ["save", "clipboard", "bank"] as const;

interface Action {
  id: string;
  group: (typeof menuGroups)[number];
  label: string;
  help: string;
  action?: EditorAction;
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
  const root = useRef<HTMLDivElement>(null);
  const { onSave, onSaveAs, onExport, onReset, onUndo, onRedo } = controller;
  const { document, onRevertToSaved } = controller;
  const save = onSave ?? onSaveAs ?? onExport;
  useEditorShortcuts(root, {
    disabled,
    target,
    save,
    undo: onUndo,
    redo: onRedo,
    run,
  });
  const actions: Action[] = [
    {
      id: "save",
      group: "save",
      label: "Save",
      help: "Save changes to the open bank",
      action: onSave,
    },
    {
      id: "save-as",
      group: "save",
      label: "Save As…",
      help: "Choose a new filename or location",
      action: onSaveAs,
    },
    {
      id: "save-with-backup",
      group: "save",
      label: "Save w/ Backup",
      help: "Save changes and keep a backup of the previous file",
      action: controller.onSaveWithBackup,
    },
    {
      id: "backup-history",
      group: "bank",
      label: controller.canRestoreBackup ? "Restore Backup…" : "Bank Backups…",
      help: "View backups for the open bank",
      action: controller.onBackupHistory,
    },
    {
      id: "resign",
      group: "bank",
      label: "Re-sign Bank",
      help: "Recalculate the open bank’s signature",
      action: controller.onResign,
    },
    {
      id: "export",
      group: "bank",
      label: "Export copy…",
      help: "Export the bank without replacing the open file",
      action: onExport,
    },
    {
      id: "clipboard",
      group: "clipboard",
      label: "Copy bank XML to clipboard",
      help: "Copy the prepared XML of the open bank",
      action: controller.onCopy,
    },
    {
      id: "revert-to-saved",
      group: "bank",
      label: "Revert to Last Save",
      help: "Restore the last saved bank in the editor",
      action:
        document?.dirty && onRevertToSaved
          ? async (target) => {
              if (
                !window.confirm(
                  "Replace the unsaved edits with the last saved bank?",
                )
              )
                return { ok: true };
              const result = await onRevertToSaved(target);
              return result.ok
                ? { ...result, message: "Reverted to the last save." }
                : result;
            }
          : undefined,
    },
  ];
  const available = actions.flatMap(({ action, ...item }) =>
    action ? [{ ...item, action }] : [],
  );
  const [primary, ...secondary] = available;
  const groupedMenu = menuGroups
    .map((group) => ({
      group,
      items: [
        ...(group === "save" && onSave
          ? [{ id: "overwrite", label: "Save (Overwrite)", action: onSave }]
          : []),
        ...secondary.filter((item) => item.group === group),
      ],
    }))
    .filter(({ items }) => items.length > 0);

  return (
    <div className="editor-file-actions" ref={root}>
      {onUndo && (
        <EditorHelpButton
          className="editor-button"
          help="Undo the last bank change"
          aria-keyshortcuts={UNDO_SHORTCUT}
          disabled={disabled}
          onClick={() => run(() => onUndo(target))}
        >
          Undo
        </EditorHelpButton>
      )}
      {onRedo && (
        <EditorHelpButton
          className="editor-button"
          help="Redo the last reverted bank change"
          aria-keyshortcuts={REDO_SHORTCUT}
          disabled={disabled}
          onClick={() => run(() => onRedo(target))}
        >
          Redo
        </EditorHelpButton>
      )}
      {onReset && (
        <EditorHelpButton
          className="editor-button editor-reset"
          help="Restore the originally opened bank values"
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
        </EditorHelpButton>
      )}
      {primary && (
        <div className="editor-save-split">
          <EditorHelpButton
            className="editor-button editor-primary"
            help={primary.help}
            aria-keyshortcuts={
              primary.action === save ? SAVE_SHORTCUT : undefined
            }
            disabled={disabled}
            onClick={() => run(() => primary.action(target))}
          >
            {primary.label}
          </EditorHelpButton>
          {groupedMenu.length > 0 && (
            <Dropdown>
              <EditorTooltip
                help="Choose how to save or copy the open bank"
                disabled={disabled}
              >
                <Dropdown.Trigger
                  className="editor-button editor-primary editor-save-menu"
                  aria-label="More save options"
                  isDisabled={disabled}
                >
                  <ChevronDown aria-hidden="true" />
                </Dropdown.Trigger>
              </EditorTooltip>
              <Dropdown.Popover
                placement="top end"
                className="editor-action-popover"
              >
                <Dropdown.Menu aria-label="Save options">
                  {groupedMenu.map(({ group, items }, index) => (
                    <Fragment key={group}>
                      {index > 0 && (
                        <Separator className="editor-menu-divider" />
                      )}
                      {items.map(({ id, label, action }) => (
                        <Dropdown.Item
                          id={id}
                          key={id}
                          onAction={() => run(() => action(target))}
                        >
                          {label}
                        </Dropdown.Item>
                      ))}
                    </Fragment>
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

export function SettingsButton({ open }: { open?: () => void }) {
  return open ? (
    <EditorHelpButton
      className="editor-button"
      aria-label="Settings"
      help="Open editor settings"
      placement="bottom"
      onClick={open}
    >
      <Settings aria-hidden="true" className="settings-icon" />
    </EditorHelpButton>
  ) : null;
}
