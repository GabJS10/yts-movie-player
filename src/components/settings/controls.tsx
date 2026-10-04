import type { ReactNode } from "react";

/** One setting: title and help on the left, the control on the right (stacked when `stack`). */
export function SetRow({
  title,
  help,
  stack = false,
  children,
  id,
}: {
  title: ReactNode;
  help?: ReactNode;
  stack?: boolean;
  children?: ReactNode;
  id?: string;
}) {
  return (
    <div className={`set-row ${stack ? "set-row-stack" : ""}`}>
      <div>
        <h3 id={id}>{title}</h3>
        {help && <p>{help}</p>}
      </div>
      {children}
    </div>
  );
}

export function SetSection({
  id,
  title,
  lede,
  soon = false,
  children,
}: {
  id: string;
  title: string;
  lede?: ReactNode;
  /** Visible but not wired yet (a later phase): controls are disabled. */
  soon?: boolean;
  children: ReactNode;
}) {
  return (
    <section className="set-section" id={id} aria-labelledby={`${id}-title`} data-set-section>
      <h2 id={`${id}-title`} className="m-0 mb-1 flex items-center gap-3 text-[22px] font-extrabold">
        {title}
        {soon && (
          <span className="rounded-sm border border-line-hi px-2 text-[11.5px] leading-[22px] font-semibold tracking-[0.08em] text-muted uppercase">
            Próximamente
          </span>
        )}
      </h2>
      {lede && <p className="m-0 mb-[22px] max-w-[64ch] text-sm text-muted">{lede}</p>}
      {soon ? (
        <fieldset disabled className="m-0 min-w-0 border-0 p-0" aria-describedby={`${id}-soon`}>
          <p id={`${id}-soon`} className="sr-only">
            Disponible en una próxima versión
          </p>
          {children}
        </fieldset>
      ) : (
        children
      )}
    </section>
  );
}

export function Switch({
  checked,
  label,
  onChange,
  disabled,
}: {
  checked: boolean;
  label: string;
  onChange?: (next: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange?.(!checked)}
    />
  );
}

export function Segmented<T extends string | number>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T | null;
  options: { value: T; label: string }[];
  onChange: (next: T) => void;
}) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          aria-pressed={o.value === value}
          onClick={() => o.value !== value && onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
