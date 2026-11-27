import { useRef } from "react";
import { MAP_PANE_WIDTH } from "../settings/settings";

const KEYBOARD_STEP = 2;

interface DragStart {
  x: number;
  percent: number;
  containerWidth: number;
  preview?: number;
}

export function SidebarSplitter({
  controls,
  disabled = false,
  percent,
  onResize,
}: {
  controls: string;
  disabled?: boolean;
  percent: number;
  onResize: (percent: number, commit: boolean) => void;
}) {
  const drag = useRef<DragStart | null>(null);

  function boundedWidth(width: number) {
    const bounded = Math.min(
      MAP_PANE_WIDTH.max,
      Math.max(MAP_PANE_WIDTH.min, width),
    );
    return Math.round(bounded * 10) / 10;
  }

  return (
    <hr
      className="sidebar-splitter"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled || undefined}
      aria-orientation="vertical"
      aria-label="Resize map list"
      aria-controls={controls}
      aria-valuemin={MAP_PANE_WIDTH.min}
      aria-valuemax={MAP_PANE_WIDTH.max}
      aria-valuenow={percent}
      onKeyDown={(event) => {
        if (disabled) return;
        switch (event.key) {
          case "ArrowLeft":
            onResize(boundedWidth(percent - KEYBOARD_STEP), true);
            break;
          case "ArrowRight":
            onResize(boundedWidth(percent + KEYBOARD_STEP), true);
            break;
          case "Home":
            onResize(MAP_PANE_WIDTH.min, true);
            break;
          case "End":
            onResize(MAP_PANE_WIDTH.max, true);
            break;
          default:
            return;
        }
        event.preventDefault();
      }}
      onPointerDown={(event) => {
        const handle = event.currentTarget;
        const containerWidth =
          handle.parentElement?.getBoundingClientRect().width;
        if (
          disabled ||
          !event.isPrimary ||
          event.button !== 0 ||
          !containerWidth
        )
          return;
        handle.setPointerCapture(event.pointerId);
        drag.current = { x: event.clientX, percent, containerWidth };
      }}
      onPointerMove={(event) => {
        if (
          !drag.current ||
          !event.currentTarget.hasPointerCapture(event.pointerId)
        )
          return;
        const { x, percent: initialPercent, containerWidth } = drag.current;
        const width = boundedWidth(
          initialPercent + ((event.clientX - x) / containerWidth) * 100,
        );
        drag.current.preview = width;
        onResize(width, false);
      }}
      onLostPointerCapture={() => {
        const finalWidth = drag.current?.preview;
        if (finalWidth !== undefined) onResize(finalWidth, true);
        drag.current = null;
      }}
    />
  );
}
