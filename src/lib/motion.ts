import { useEffect, useState } from "react";

const QUERY = "(prefers-reduced-motion: reduce)";

/** The user asked the system for less motion (no auto-advance, no fades). */
export const prefersReducedMotion = () => window.matchMedia?.(QUERY).matches ?? false;

/** prefersReducedMotion, live. */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(prefersReducedMotion);
  useEffect(() => {
    const mq = window.matchMedia?.(QUERY);
    if (!mq) return;
    const on = () => setReduced(mq.matches);
    mq.addEventListener?.("change", on);
    return () => mq.removeEventListener?.("change", on);
  }, []);
  return reduced;
}

/** The window is hidden (minimised, another workspace): nothing should animate or advance. */
export function usePageHidden(): boolean {
  const [hidden, setHidden] = useState(() => document.visibilityState === "hidden");
  useEffect(() => {
    const on = () => setHidden(document.visibilityState === "hidden");
    document.addEventListener("visibilitychange", on);
    return () => document.removeEventListener("visibilitychange", on);
  }, []);
  return hidden;
}
