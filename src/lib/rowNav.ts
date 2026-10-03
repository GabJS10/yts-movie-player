import type { KeyboardEvent } from "react";

/**
 * Keyboard navigation across poster rows: ←/→ move within a row, ↑/↓ jump to the same position
 * in the previous/next row. Rows are any element with [data-row-track]; items are its [data-card].
 */
export function onRowKeyDown(e: KeyboardEvent<HTMLElement>) {
  const track = e.currentTarget;
  const items = [...track.querySelectorAll<HTMLElement>("[data-card]")];
  const i = items.indexOf(document.activeElement as HTMLElement);
  if (i < 0) return;

  let target: HTMLElement | undefined;
  if (e.key === "ArrowRight") target = items[i + 1];
  else if (e.key === "ArrowLeft") target = items[i - 1];
  else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    const tracks = [...document.querySelectorAll<HTMLElement>("[data-row-track]")];
    const next = tracks[tracks.indexOf(track) + (e.key === "ArrowDown" ? 1 : -1)];
    if (next) {
      const nextItems = [...next.querySelectorAll<HTMLElement>("[data-card]")];
      // Same visual column: count from the first fully visible card of each row.
      const firstVisible = (list: HTMLElement[], container: HTMLElement) =>
        Math.max(
          0,
          list.findIndex((el) => el.offsetLeft + 1 >= container.scrollLeft),
        );
      const column = i - firstVisible(items, track);
      target = nextItems[Math.min(nextItems.length - 1, firstVisible(nextItems, next) + column)];
    }
  } else return;

  if (target) {
    e.preventDefault();
    target.focus({ preventScroll: true });
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    target.scrollIntoView?.({ block: "nearest", inline: "nearest", behavior: reduce ? "auto" : "smooth" });
  }
}
