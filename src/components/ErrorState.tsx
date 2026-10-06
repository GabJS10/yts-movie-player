import { Link } from "@tanstack/react-router";
import { describeError, ERROR_NEXT } from "../api/errors";
import type { AppError } from "../api/types";
import { useT } from "../i18n";
import { Icon } from "./Icon";

type Props = {
  error: AppError;
  onRetry?: () => void;
  /** A way back that fits the place (e.g. the player back to the movie page). */
  onBack?: () => void;
  backLabel?: string;
  compact?: boolean;
};

/**
 * Error with its explanation and the next actions for its code (ERROR_NEXT): retry when it can
 * help, and where to fix it. Never shows `message` (technical, English).
 */
export function ErrorState({ error, onRetry, onBack, backLabel, compact = false }: Props) {
  const t = useT();
  const copy = describeError(error);
  const next = ERROR_NEXT[error.code];
  const retry = next.retry && onRetry;
  const btn = "btn btn-line btn-sm";
  return (
    <div
      role="alert"
      data-testid="error-state"
      data-code={error.code}
      className={`grid max-w-[520px] justify-items-start gap-3 ${compact ? "py-4" : "py-16"}`}
    >
      <h2 className={`m-0 font-extrabold ${compact ? "text-base" : "text-[22px]"}`}>{copy.title}</h2>
      <p className="m-0 text-muted">{copy.action}</p>
      <div className="flex flex-wrap gap-2.5">
        {retry && (
          <button type="button" className="btn btn-line btn-sm" data-testid="error-retry" onClick={onRetry}>
            <Icon name="refresh" size={18} />
            {t.common.retry}
          </button>
        )}
        {next.link && (
          <Link to={next.link.to} hash={next.link.hash} className={btn} data-testid="error-link">
            {t.errorLinks[next.link.label]}
          </Link>
        )}
        {onBack && (
          <button type="button" className={btn} data-testid="error-back" onClick={onBack}>
            <Icon name="back" size={18} />
            {backLabel ?? t.common.back}
          </button>
        )}
      </div>
    </div>
  );
}
