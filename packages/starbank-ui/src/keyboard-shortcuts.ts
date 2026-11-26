import { type RefObject, useEffect, useEffectEvent } from "react";
import type { EditorAction, EditorTarget } from "starbank-editor";
import type { RunEditorAction } from "./editor-actions";

export const SAVE_SHORTCUT = "Control+S Meta+S";
export const UNDO_SHORTCUT = "Control+Z Meta+Z";
export const REDO_SHORTCUT = "Control+Shift+Z Meta+Shift+Z Control+Y";
export const SEARCH_SHORTCUT = "Control+F Meta+F";

const OVERLAY =
  'dialog[open], [role="dialog"], [role="alertdialog"], [role="menu"]';
const TEXT_EDITOR =
  'input, textarea, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="spinbutton"]';

function usePrimaryShortcut(
  root: RefObject<HTMLElement | null>,
  handler: (event: KeyboardEvent, focused: Element | null) => void,
) {
  const handle = useEffectEvent(handler);
  useEffect(() => {
    function dispatch(event: KeyboardEvent) {
      if (
        !root.current ||
        root.current.closest("[hidden], [inert]") ||
        event.defaultPrevented ||
        event.isComposing ||
        event.altKey ||
        event.ctrlKey === event.metaKey
      ) {
        return;
      }
      const focused =
        event.target instanceof Element ? event.target : document.activeElement;
      if (!focused?.closest(OVERLAY)) handle(event, focused);
    }

    window.addEventListener("keydown", dispatch);
    return () => window.removeEventListener("keydown", dispatch);
  }, [root]);
}

export function useEditorShortcuts(
  root: RefObject<HTMLElement | null>,
  {
    disabled,
    target,
    save,
    undo,
    redo,
    run,
  }: {
    disabled: boolean;
    target: EditorTarget;
    save?: EditorAction;
    undo?: EditorAction;
    redo?: EditorAction;
    run: RunEditorAction;
  },
) {
  usePrimaryShortcut(root, (event, focused) => {
    const key = event.key.toLowerCase();
    let action: EditorAction | undefined;
    if (key === "s" && !event.shiftKey) {
      action = save;
    } else if (!focused?.closest(TEXT_EDITOR)) {
      if (key === "z") action = event.shiftKey ? redo : undo;
      if (key === "y" && event.ctrlKey && !event.shiftKey) action = redo;
    }

    if (!action) return;
    const selected = action;
    event.preventDefault();
    if (!event.repeat && !disabled) run(() => selected(target));
  });
}

export function useMapSearchShortcut(
  input: RefObject<HTMLInputElement | null>,
) {
  usePrimaryShortcut(input, (event, focused) => {
    if (
      input.current &&
      !input.current.disabled &&
      !event.shiftKey &&
      event.key.toLowerCase() === "f" &&
      !focused?.closest(".cm-editor")
    ) {
      event.preventDefault();
      if (!event.repeat) {
        input.current.focus();
        input.current.select();
      }
    }
  });
}
