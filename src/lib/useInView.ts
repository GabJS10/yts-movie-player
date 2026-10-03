import { useCallback, useEffect, useState } from "react";

/**
 * True while the element is within `rootMargin` of its scroll container (or the viewport).
 * Returns a callback ref, so it also works for elements that mount later (e.g. a pagination sentinel).
 */
export function useInView<T extends Element>(options: { root?: Element | null; rootMargin?: string } = {}) {
  const [el, setEl] = useState<T | null>(null);
  const [inView, setInView] = useState(false);
  const { root = null, rootMargin = "0px" } = options;
  const ref = useCallback((node: T | null) => setEl(node), []);
  useEffect(() => {
    // Without an element the last reading is kept; callers also gate on their own state (e.g. hasNextPage).
    if (!el || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver((entries) => setInView(entries.some((e) => e.isIntersecting)), {
      root,
      rootMargin,
    });
    io.observe(el);
    return () => io.disconnect();
  }, [el, root, rootMargin]);
  return [ref, inView] as const;
}
