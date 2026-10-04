import { Link } from "@tanstack/react-router";
import { describeError } from "../api/errors";
import type { AppError } from "../api/types";
import { Icon } from "./Icon";

type Props = { error: AppError; onRetry?: () => void; compact?: boolean };

/** Error with its Spanish explanation and a next action. Never shows `message` (technical, English). */
export function ErrorState({ error, onRetry, compact = false }: Props) {
  const copy = describeError(error);
  const goHome = error.code === "not_found";
  return (
    <div
      role="alert"
      data-testid="error-state"
      data-code={error.code}
      className={`grid max-w-[520px] justify-items-start gap-3 ${compact ? "py-4" : "py-16"}`}
    >
      <h2 className={`m-0 font-extrabold ${compact ? "text-base" : "text-[22px]"}`}>{copy.title}</h2>
      <p className="m-0 text-muted">{copy.action}</p>
      {goHome ? (
        <Link to="/" className="btn btn-line btn-sm">
          Volver al inicio
        </Link>
      ) : (
        onRetry && (
          <button type="button" className="btn btn-line btn-sm" onClick={onRetry}>
            <Icon name="refresh" size={18} />
            Reintentar
          </button>
        )
      )}
    </div>
  );
}
