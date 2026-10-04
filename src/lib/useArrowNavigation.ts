import { useEffect } from "react";
import { prefersReducedMotion } from "./motion";

// Arrow keys over the posters, app-wide. Inside a row, MovieRow's own handler (rowNav.ts) moves the
// focus; this covers what it can't (phase 2 bug, seen in the WebKitGTK window):
// - Nothing focused (focus on <body> after loading, or after a mouse click: WebKit doesn't focus links on
//   click), so the arrows only scrolled the page. Now the first arrow enters the posters: the card under
//   the pointer, else the first card of the first row on screen.
// - Grids of cards (Buscar, Mi lista): ←/→ previous/next, ↑/↓ the same column in the line above/below.
// - ↑ from the first row goes to the banner's Reproducir; ↓ from the banner, to the first row.

const ARROWS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"]);
const CARD = "[data-card]";
const CONTAINER = "[data-row-track], .grid-cards";

/** Where arrows belong to the control (text, sliders, menus, dialogs, the player, the versions table). */
const OWNS_ARROWS =
  "input, textarea, select, [contenteditable], [role=slider], [role=radiogroup], [role=radio], [role=menu], [role=menuitemradio], [role=dialog], [data-player]";

let lastHovered: HTMLElement | null = null;

const visible = (el: Element) => {
  const r = el.getBoundingClientRect();
  return r.bottom > 0 && r.top < window.innerHeight && r.right > 0 && r.left < window.innerWidth;
};

export function focusCard(el: HTMLElement | null | undefined): boolean {
  if (!el) return false;
  el.focus({ preventScroll: true });
  el.scrollIntoView?.({
    block: "nearest",
    inline: "nearest",
    behavior: prefersReducedMotion() ? "auto" : "smooth",
  });
  return true;
}

/** The first card of the first row or grid on screen (top to bottom). */
function firstVisibleCard(): HTMLElement | null {
  for (const container of document.querySelectorAll<HTMLElement>(CONTAINER)) {
    if (!visible(container)) continue;
    const cards = [...container.querySelectorAll<HTMLElement>(CARD)];
    const card = cards.find(visible) ?? cards[0];
    if (card) return card;
  }
  return document.querySelector<HTMLElement>(CARD);
}

/** The card in `cards` on the line above/below `from` (same offsetTop = same line), nearest column. */
export function gridNeighbour(cards: HTMLElement[], from: HTMLElement, dir: 1 | -1): HTMLElement | undefined {
  const lines = [...new Set(cards.map((c) => c.offsetTop))].sort((a, b) => a - b);
  const line = lines[lines.indexOf(from.offsetTop) + dir];
  if (line === undefined) return undefined;
  const candidates = cards.filter((c) => c.offsetTop === line);
  return candidates.reduce((best, c) =>
    Math.abs(c.offsetLeft - from.offsetLeft) < Math.abs(best.offsetLeft - from.offsetLeft) ? c : best,
  );
}

export function handleArrow(e: KeyboardEvent): void {
  if (e.defaultPrevented || !ARROWS.has(e.key) || e.altKey || e.ctrlKey || e.metaKey) return;
  const active = document.activeElement as HTMLElement | null;
  if (active?.closest(OWNS_ARROWS)) return;

  const card = active?.closest<HTMLElement>(CARD) ?? null;
  const container = card?.closest<HTMLElement>(CONTAINER) ?? null;
  const hero = active?.closest<HTMLElement>("[data-testid=hero]") ?? null;
  let target: HTMLElement | null | undefined = null;

  if (card && container?.matches(".grid-cards")) {
    const cards = [...container.querySelectorAll<HTMLElement>(CARD)];
    const i = cards.indexOf(card);
    if (e.key === "ArrowRight") target = cards[i + 1];
    else if (e.key === "ArrowLeft") target = cards[i - 1];
    else target = gridNeighbour(cards, card, e.key === "ArrowDown" ? 1 : -1);
  } else if (card && container && e.key === "ArrowUp") {
    // The row handler found no row above: the banner, if there is one.
    const tracks = [...document.querySelectorAll(CONTAINER)];
    if (tracks.indexOf(container) === 0)
      target = document.querySelector<HTMLElement>("[data-testid=hero-play]");
  } else if (hero && e.key === "ArrowDown") {
    target = firstVisibleCard() ?? document.querySelector<HTMLElement>(CARD);
  } else if (!card && !hero && (!active || active === document.body || active.matches("main, #main"))) {
    target = lastHovered?.isConnected && visible(lastHovered) ? lastHovered : firstVisibleCard();
  }

  if (target) {
    e.preventDefault();
    focusCard(target);
  }
}

function onPointerOver(e: Event) {
  const card = (e.target as Element | null)?.closest?.<HTMLElement>(CARD);
  if (card) lastHovered = card;
}

/** Mounted once, in the app shell. */
export function useArrowNavigation() {
  useEffect(() => {
    document.addEventListener("keydown", handleArrow);
    document.addEventListener("pointerover", onPointerOver, { passive: true });
    return () => {
      document.removeEventListener("keydown", handleArrow);
      document.removeEventListener("pointerover", onPointerOver);
    };
  }, []);
}
