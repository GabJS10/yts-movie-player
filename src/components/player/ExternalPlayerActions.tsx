import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { describeError } from "../../api/errors";
import { openExternalPlayer, toAppError } from "../../api/tauri";
import type { AppError, ExternalPlayerResult, ExternalSubtitleArgs, Torrent } from "../../api/types";
import { formatResetTime } from "../../lib/subtitles";
import { onQuotaExhausted } from "../../store/subtitlesQuota";
import { Icon } from "../Icon";

type Props = {
  movieId: number;
  infohash: string;
  alternative: Torrent | null;
  primary?: boolean;
  /** The subtitle active in the player and its delay; empty = the backend picks one per Ajustes. */
  subtitles?: ExternalSubtitleArgs;
};

type SubtitleOutcome = ExternalPlayerResult["subtitle"];

type Note = { text: string; /** Ajustes section that fixes it. */ settings: "s-subs" | "s-play" | null };

/** Why VLC opened without subtitles. null for "loaded" / "none" (nothing to say). */
function subtitleNote(outcome: SubtitleOutcome, resetAt: string | null): Note | null {
  switch (outcome) {
    case "no_key":
      return { text: "VLC se abrió sin subtítulos: falta la clave de OpenSubtitles.", settings: "s-subs" };
    case "quota": {
      const at = formatResetTime(resetAt);
      return {
        text: `VLC se abrió sin subtítulos: se agotó el cupo diario de OpenSubtitles${at ? ` (se renueva a las ${at})` : ""}.`,
        settings: null,
      };
    }
    case "not_found":
      return {
        text: "VLC se abrió sin subtítulos: no encontramos ninguno para esta película.",
        settings: null,
      };
    case "unsupported_player":
      return {
        text: "Tu reproductor externo no admite que le pasemos subtítulos: se abrió sin ellos.",
        settings: "s-play",
      };
    case "error":
      return { text: "VLC se abrió, pero no se pudieron cargar los subtítulos.", settings: null };
    default:
      return null;
  }
}

/**
 * "Abrir en VLC" + "Cambiar a 1080p x264". VLC gets the active subtitle; the external-player error
 * and any subtitle problem (VLC opens anyway) are explained inline.
 */
export function ExternalPlayerActions({ movieId, infohash, alternative, primary = true, subtitles }: Props) {
  const [error, setError] = useState<AppError | null>(null);
  const [note, setNote] = useState<Note | null>(null);
  const [opening, setOpening] = useState(false);
  const open = async () => {
    setOpening(true);
    setError(null);
    setNote(null);
    try {
      const { subtitle } = await openExternalPlayer(infohash, subtitles);
      const resetAt = subtitle === "quota" ? await onQuotaExhausted() : null;
      setNote(subtitleNote(subtitle, resetAt));
    } catch (err) {
      setError(toAppError(err));
    } finally {
      setOpening(false);
    }
  };
  return (
    <div>
      <div className="flex flex-wrap gap-3">
        <button
          type="button"
          className={`btn ${primary ? "btn-play" : "btn-line btn-sm"}`}
          onClick={() => void open()}
          disabled={opening}
        >
          <Icon name="external" size={primary ? 22 : 18} />
          Abrir en VLC
        </button>
        {alternative && (
          <Link
            to="/play/$movieId"
            params={{ movieId }}
            search={{ infohash: alternative.infohash }}
            replace
            className={`btn btn-line ${primary ? "" : "btn-sm"}`}
          >
            Cambiar a {alternative.quality} x264
          </Link>
        )}
      </div>
      {error && (
        <p role="alert" className="mt-3 text-sm text-warn">
          {describeError(error).title}. {describeError(error).action}
        </p>
      )}
      {note && (
        <p role="status" className="mt-3 text-sm text-text-2">
          {note.text}{" "}
          {note.settings && (
            <Link to="/settings" hash={note.settings} className="font-semibold text-green hover:underline">
              Ir a Ajustes
            </Link>
          )}
        </p>
      )}
    </div>
  );
}
