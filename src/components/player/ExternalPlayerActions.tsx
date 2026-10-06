import { Link } from "@tanstack/react-router";
import { describeError } from "../../api/errors";
import type { ExternalSubtitleArgs, Torrent } from "../../api/types";
import { Icon } from "../Icon";
import { useOpenExternal } from "./useOpenExternal";

type Props = {
  movieId: number;
  infohash: string;
  alternative: Torrent | null;
  primary?: boolean;
  /** The subtitle active in the player and its delay; empty = the backend picks one per Ajustes. */
  subtitles?: ExternalSubtitleArgs;
};

/**
 * "Abrir en VLC" + "Cambiar a 1080p x264". VLC gets the active subtitle; the external-player error
 * and any subtitle problem (VLC opens anyway) are explained inline.
 */
export function ExternalPlayerActions({ movieId, infohash, alternative, primary = true, subtitles }: Props) {
  const { open, opening, error, note } = useOpenExternal({ infohash, subtitles });
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
