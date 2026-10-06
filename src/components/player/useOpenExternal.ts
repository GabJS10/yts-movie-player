import { useCallback, useState } from "react";
import { openExternalPlayer, toAppError } from "../../api/tauri";
import type { AppError, ExternalPlayerResult, ExternalSubtitleArgs } from "../../api/types";
import { formatResetTime } from "../../lib/subtitles";
import { onQuotaExhausted } from "../../store/subtitlesQuota";

type SubtitleOutcome = ExternalPlayerResult["subtitle"];

export type ExternalNote = {
  text: string;
  /** Ajustes section that fixes it. */
  settings: "s-subs" | "s-play" | null;
};

/** Why VLC opened without subtitles. null for "loaded" / "none" (nothing to say). */
function subtitleNote(outcome: SubtitleOutcome, resetAt: string | null): ExternalNote | null {
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
 * Opens the stream in the external player (VLC) with the active subtitle. Keeps the outcome: the
 * external-player error, or a note when VLC opened without subtitles.
 */
export function useOpenExternal({
  infohash,
  subtitles,
}: {
  infohash: string;
  subtitles?: ExternalSubtitleArgs;
}) {
  const [error, setError] = useState<AppError | null>(null);
  const [note, setNote] = useState<ExternalNote | null>(null);
  const [opening, setOpening] = useState(false);
  const open = useCallback(async () => {
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
  }, [infohash, subtitles]);
  const dismiss = useCallback(() => {
    setError(null);
    setNote(null);
  }, []);
  return { open, opening, error, note, dismiss };
}
