import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { describeError } from "../../api/errors";
import { openExternalPlayer, toAppError } from "../../api/tauri";
import type { AppError, Torrent } from "../../api/types";
import { Icon } from "../Icon";

type Props = { movieId: number; infohash: string; alternative: Torrent | null; primary?: boolean };

/** "Abrir en VLC" + "Cambiar a 1080p x264", with the external-player error explained inline. */
export function ExternalPlayerActions({ movieId, infohash, alternative, primary = true }: Props) {
  const [error, setError] = useState<AppError | null>(null);
  const [opening, setOpening] = useState(false);
  const open = async () => {
    setOpening(true);
    setError(null);
    try {
      await openExternalPlayer(infohash);
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
    </div>
  );
}
