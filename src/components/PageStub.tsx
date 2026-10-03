import type { ReactNode } from "react";

/** Placeholder for routes whose content lands in a later phase (docs/ROADMAP.md). */
export function PageStub({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="min-h-screen px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-24">
      <h1 className="m-0 text-headline font-[850] uppercase stretch-condensed">{title}</h1>
      {children && <div className="mt-2 max-w-[64ch] text-muted">{children}</div>}
    </div>
  );
}
