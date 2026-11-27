import { ChevronRight } from "lucide-react";
import { useLayoutEffect, useRef } from "react";

const TAIL_TOLERANCE = 1;
const SCROLL_STEP = 48;

function scrollLimit(viewport: HTMLElement) {
  return Math.max(0, viewport.scrollWidth - viewport.clientWidth);
}

export function BankBreadcrumbs({ segments }: { segments: readonly string[] }) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLOListElement>(null);
  const path = JSON.stringify(segments);

  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    const list = listRef.current;
    if (!viewport || !list) return;

    let followingTail = true;
    let end = 0;
    let position = 0;

    const resize = () => {
      const limit = scrollLimit(viewport);
      if (!viewport.clientWidth || limit === end) return;
      if (followingTail) viewport.scrollLeft = limit;
      end = limit;
      position = viewport.scrollLeft;
    };

    const scroll = () => {
      if (!viewport.clientWidth) return;
      if (scrollLimit(viewport) !== end) {
        // A resize can clamp the position and dispatch scroll before ResizeObserver.
        resize();
      } else if (viewport.scrollLeft !== position) {
        position = viewport.scrollLeft;
        followingTail = position >= end - TAIL_TOLERANCE;
      }
    };

    const scrollWithKeys = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }

      let offset: number;
      switch (event.key) {
        case "Home":
          offset = 0;
          break;
        case "End":
          offset = scrollLimit(viewport);
          break;
        case "ArrowLeft":
          offset = viewport.scrollLeft - SCROLL_STEP;
          break;
        case "ArrowRight":
          offset = viewport.scrollLeft + SCROLL_STEP;
          break;
        default:
          return;
      }
      event.preventDefault();
      resize();
      viewport.scrollLeft = offset;
      position = viewport.scrollLeft;
      followingTail = position >= end - TAIL_TOLERANCE;
    };

    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(viewport);
    observer.observe(list);
    viewport.addEventListener("scroll", scroll, { passive: true });
    viewport.addEventListener("keydown", scrollWithKeys);
    return () => {
      observer.disconnect();
      viewport.removeEventListener("scroll", scroll);
      viewport.removeEventListener("keydown", scrollWithKeys);
    };
  }, [path]);

  let offset = 0;
  return (
    // biome-ignore lint/a11y/useSemanticElements: This is a named scroll group without form controls.
    <div
      aria-label="Bank file path"
      className="bank-breadcrumbs"
      dir="ltr"
      ref={viewportRef}
      role="group"
      // biome-ignore lint/a11y/noNoninteractiveTabindex: Keyboard users must be able to focus and scroll the path.
      tabIndex={0}
    >
      <ol
        className="bank-breadcrumbs__path"
        ref={listRef}
        // biome-ignore lint/a11y/noRedundantRoles: Safari drops list semantics for lists without markers.
        role="list"
      >
        {segments.map((segment, index) => {
          const key = offset;
          offset += segment.length + 1;
          return (
            <li
              aria-current={
                index === segments.length - 1 ? "location" : undefined
              }
              key={key}
            >
              {index !== 0 && <ChevronRight aria-hidden="true" />}
              <bdi>{segment}</bdi>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
