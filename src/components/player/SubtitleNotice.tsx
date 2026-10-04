import { Link } from "@tanstack/react-router";
import { useEffect, type ReactNode } from "react";
import { describeError } from "../../api/errors";
import { FALLBACK_LANG, formatResetTime, langLabel } from "../../lib/subtitles";
import { Icon } from "../Icon";
import type { SubtitleNotice as Notice } from "./useSubtitles";

const AUTO_HIDE_MS = 12_000;

type Props = {
  notice: Notice;
  onDismiss: () => void;
  onFallback: () => void;
  onPickFile: () => void;
};

const action = "font-semibold text-green hover:underline";

/** Discreet, dismissible line at the top of the player: what happened with subtitles and what to do. */
export function SubtitleNotice({ notice, onDismiss, onFallback, onPickFile }: Props) {
  useEffect(() => {
    const id = window.setTimeout(onDismiss, AUTO_HIDE_MS);
    return () => window.clearTimeout(id);
  }, [notice, onDismiss]);

  const settingsLink = (
    <Link to="/settings" hash="s-subs" className={action}>
      Ir a Ajustes › Subtítulos
    </Link>
  );
  const fileButton = (
    <button type="button" className={action} onClick={onPickFile}>
      Cargar un archivo
    </button>
  );

  let text: ReactNode;
  let actions: ReactNode = null;
  switch (notice.kind) {
    case "no-key":
      text = "Los subtítulos no se buscan solos: falta la clave de OpenSubtitles.";
      actions = settingsLink;
      break;
    case "auth":
      text = "OpenSubtitles no acepta la clave guardada.";
      actions = settingsLink;
      break;
    case "none":
      text = `No hay subtítulos en ${langLabel(notice.lang).toLowerCase()} para esta película.`;
      actions =
        notice.lang !== FALLBACK_LANG ? (
          <>
            <button type="button" className={action} onClick={onFallback}>
              Usar inglés
            </button>
            {fileButton}
          </>
        ) : (
          fileButton
        );
      break;
    case "quota": {
      const at = formatResetTime(notice.resetAt);
      text = `Se agotó el cupo diario de OpenSubtitles${at ? `; se renueva a las ${at}` : ""}.`;
      actions = fileButton;
      break;
    }
    case "error":
      text = `${describeError(notice.error).title}. No se pudieron cargar los subtítulos.`;
      actions = fileButton;
      break;
  }

  return (
    <div
      role="status"
      className="absolute top-[84px] left-1/2 z-10 flex w-max max-w-[calc(100%-32px)] -translate-x-1/2 flex-wrap items-center gap-x-4 gap-y-1.5 rounded-lg bg-black/80 py-2.5 pr-2.5 pl-4 text-sm text-text-2 shadow-[0_12px_32px_rgba(0,0,0,.5)]"
      data-testid="subtitle-notice"
    >
      <Icon name="cc" size={18} className="flex-none text-muted" />
      <span>{text}</span>
      {actions}
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
