import { Link } from "@tanstack/react-router";
import { useEffect } from "react";
import { describeError } from "../../api/errors";
import type { AppError } from "../../api/types";
import { Icon } from "../Icon";
import type { ExternalNote } from "./useOpenExternal";

const AUTO_HIDE_MS = 12_000;

type Props = {
  error: AppError | null;
  note: ExternalNote | null;
  onDismiss: () => void;
  /** Below the subtitle notice when both are up. */
  lower?: boolean;
};

const action = "font-semibold text-green hover:underline";

/** Discreet, dismissible line in the player: VLC is missing, or it opened without subtitles. */
export function ExternalNotice({ error, note, onDismiss, lower = false }: Props) {
  useEffect(() => {
    const id = window.setTimeout(onDismiss, AUTO_HIDE_MS);
    return () => window.clearTimeout(id);
  }, [error, note, onDismiss]);

  const settings = error?.code === "external_player_missing" ? "s-play" : (note?.settings ?? null);
  return (
    <div
      role={error ? "alert" : "status"}
      className={`absolute ${lower ? "top-[140px]" : "top-[84px]"} left-1/2 z-10 flex w-max max-w-[calc(100%-32px)] -translate-x-1/2 flex-wrap items-center gap-x-4 gap-y-1.5 rounded-lg bg-black/80 py-2.5 pr-2.5 pl-4 text-sm text-text-2 shadow-[0_12px_32px_rgba(0,0,0,.5)]`}
      data-testid="external-notice"
    >
      <Icon name="external" size={18} className={`flex-none ${error ? "text-warn" : "text-muted"}`} />
      <span>{error ? `${describeError(error).title}. ${describeError(error).action}` : note?.text}</span>
      {settings && (
        <Link to="/settings" hash={settings} className={action}>
          Ir a Ajustes
        </Link>
      )}
      <button
        type="button"
        className="inline-grid size-7 place-items-center rounded-full text-muted hover:bg-white/10 hover:text-text"
        aria-label="Cerrar aviso"
        onClick={onDismiss}
      >
        <Icon name="x" size={16} />
      </button>
    </div>
  );
}
