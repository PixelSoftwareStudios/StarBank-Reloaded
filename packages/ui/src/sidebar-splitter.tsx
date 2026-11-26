import { useRef } from "react";

export const DEFAULT_MAP_PANE_WIDTH = 28;
const MIN_MAP_PANE_WIDTH = 20;
const MAX_MAP_PANE_WIDTH = 50;
const KEYBOARD_STEP = 2;

interface DragStart {
  x: number;
  percent: number;
  containerWidth: number;
}

export function SidebarSplitter({
  controls,
  percent,
  onResize,
}: {
  controls: string;
  percent: number;
  onResize: (percent: number) => void;
}) {
  const drag = useRef<DragStart | null>(null);

  function updateWidth(width: number) {
    const bounded = Math.min(
      MAX_MAP_PANE_WIDTH,
      Math.max(MIN_MAP_PANE_WIDTH, width),
    );
    onResize(Math.round(bounded * 10) / 10);
  }

  return (
    <hr
      className="sidebar-splitter"
      tabIndex={0}
      aria-orientation="vertical"
      aria-label="Resize map list"
      aria-controls={controls}
      aria-valuemin={MIN_MAP_PANE_WIDTH}
      aria-valuemax={MAX_MAP_PANE_WIDTH}
      aria-valuenow={percent}
      onKeyDown={(event) => {
        switch (event.key) {
          case "ArrowLeft":
            updateWidth(percent - KEYBOARD_STEP);
            break;
          case "ArrowRight":
            updateWidth(percent + KEYBOARD_STEP);
            break;
          case "Home":
            updateWidth(MIN_MAP_PANE_WIDTH);
            break;
          case "End":
            updateWidth(MAX_MAP_PANE_WIDTH);
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
        if (!event.isPrimary || event.button !== 0 || !containerWidth) return;
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
        updateWidth(
          initialPercent + ((event.clientX - x) / containerWidth) * 100,
        );
      }}
      onLostPointerCapture={() => {
        drag.current = null;
      }}
    />
  );
}
