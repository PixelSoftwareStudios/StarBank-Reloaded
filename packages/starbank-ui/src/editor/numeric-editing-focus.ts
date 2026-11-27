import {
  type ComponentProps,
  useEffect,
  useEffectEvent,
  useRef,
  useState,
} from "react";

interface FocusedRow {
  rowId: string;
  exact: boolean;
}

interface FocusTransition {
  rowId: string;
  field: HTMLInputElement;
  grouped: string;
  exact: string;
}

type FocusHandlers = Pick<
  ComponentProps<"input">,
  | "onPointerDown"
  | "onFocus"
  | "onBlur"
  | "onKeyDown"
  | "onCompositionStart"
  | "onCompositionEnd"
>;

function exactOffset(grouped: string, offset: number): number {
  let exact = 0;
  for (let index = 0; index < offset; index += 1) {
    if (grouped[index] !== ",") exact += 1;
  }
  return exact;
}

export function useNumericEditingFocus() {
  const [focused, setFocused] = useState<FocusedRow>();
  const pointer = useRef<{ field: HTMLInputElement; id: number } | null>(null);
  const pending = useRef<FocusTransition | null>(null);
  const frame = useRef<number | undefined>(undefined);

  function cancelTransition(field: HTMLInputElement) {
    if (pending.current?.field !== field) return;
    pending.current = null;
    if (frame.current !== undefined) {
      cancelAnimationFrame(frame.current);
      frame.current = undefined;
    }
  }

  function revealExact(transition: FocusTransition) {
    if (pending.current !== transition) return;
    const { field, rowId, grouped, exact } = transition;
    cancelTransition(field);
    if (field.ownerDocument.activeElement !== field || field.value !== grouped)
      return;

    const start = exactOffset(grouped, field.selectionStart ?? 0);
    const end = exactOffset(grouped, field.selectionEnd ?? 0);
    const direction = field.selectionDirection ?? undefined;
    // Set the DOM selection before the browser handles a pending key's edit.
    field.value = exact;
    field.setSelectionRange(start, end, direction);
    setFocused({ rowId, exact: true });
  }

  function schedule(transition: FocusTransition) {
    if (frame.current !== undefined) cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(() => {
      frame.current = undefined;
      revealExact(transition);
    });
  }

  const finishPointer = useEffectEvent((event: PointerEvent) => {
    if (pointer.current?.id !== event.pointerId) return;
    const { field } = pointer.current;
    pointer.current = null;
    const transition = pending.current;
    // The browser places the caret or completes the dragged selection first.
    if (transition?.field === field) schedule(transition);
  });

  useEffect(() => {
    function ended(event: PointerEvent) {
      finishPointer(event);
    }
    window.addEventListener("pointerup", ended, true);
    window.addEventListener("pointercancel", ended, true);
    return () => {
      window.removeEventListener("pointerup", ended, true);
      window.removeEventListener("pointercancel", ended, true);
      if (frame.current !== undefined) {
        cancelAnimationFrame(frame.current);
        frame.current = undefined;
      }
      pointer.current = null;
      pending.current = null;
    };
  }, []);

  function handlers(
    rowId: string,
    grouped: string,
    exact: string,
  ): FocusHandlers {
    function focus(field: HTMLInputElement, finishingComposition = false) {
      const changesDisplay = grouped !== exact && field.value === grouped;
      setFocused({ rowId, exact: !changesDisplay });
      if (changesDisplay) {
        const transition = { rowId, field, grouped, exact };
        pending.current = transition;
        if (pointer.current?.field === field) return;
        if (finishingComposition) schedule(transition);
        else revealExact(transition);
      }
    }

    return {
      onPointerDown(event) {
        const field = event.currentTarget;
        if (
          event.button === 0 &&
          event.isPrimary &&
          grouped !== exact &&
          field !== field.ownerDocument.activeElement
        ) {
          pointer.current = { field, id: event.pointerId };
        }
      },
      onFocus(event) {
        focus(event.currentTarget);
      },
      onBlur(event) {
        const field = event.currentTarget;
        if (pointer.current?.field === field) pointer.current = null;
        cancelTransition(field);
        setFocused(undefined);
      },
      onKeyDown(event) {
        const transition = pending.current;
        if (!transition || transition.field !== event.currentTarget) return;
        if (event.nativeEvent.isComposing || event.key === "Process") {
          cancelTransition(transition.field);
        } else if (
          event.key !== "Tab" &&
          !["Alt", "Control", "Meta", "Shift"].includes(event.key)
        ) {
          revealExact(transition);
        }
      },
      onCompositionStart(event) {
        cancelTransition(event.currentTarget);
      },
      onCompositionEnd(event) {
        const field = event.currentTarget;
        if (
          field.ownerDocument.activeElement === field &&
          grouped !== exact &&
          field.value === grouped
        ) {
          focus(field, true);
        }
      },
    };
  }

  function inputChanged(rowId: string, field: HTMLInputElement) {
    cancelTransition(field);
    if (field.ownerDocument.activeElement === field) {
      setFocused({ rowId, exact: true });
    }
  }

  return { focused, handlers, inputChanged };
}
