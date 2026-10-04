import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";

type Props = {
  labelledBy: string;
  describedBy?: string;
  /** Esc and a click on the backdrop. */
  onClose: () => void;
  children: ReactNode;
};

/**
 * Dialog over an 82 % black backdrop (DESIGN.md). Focus goes to the element marked `data-autofocus`
 * (else the first button), Tab stays inside, and focus returns to the opener on close.
 */
export function Modal({ labelledBy, describedBy, onClose, children }: Props) {
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    const el = box.current;
    (el?.querySelector<HTMLElement>("[data-autofocus]") ?? el?.querySelector<HTMLElement>("button"))?.focus();
    return () => opener?.focus?.();
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key !== "Tab") return;
    const items = [...(box.current?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? [])];
    const first = items[0];
    const last = items.at(-1);
    if (!first || !last) return;
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        ref={box}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        onKeyDown={onKeyDown}
      >
        {children}
      </div>
    </div>
  );
}
