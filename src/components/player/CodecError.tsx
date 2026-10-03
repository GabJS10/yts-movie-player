import { Link } from "@tanstack/react-router";
import type { Torrent } from "../../api/types";
import { ReleaseTag } from "../ReleaseTag";
import { ExternalPlayerActions } from "./ExternalPlayerActions";

type Props = { movieId: number; torrent: Torrent; alternative: Torrent | null };

/** WebKitGTK can't decode this file (typically x265/HEVC): offer VLC and an x264 version. */
export function CodecError({ movieId, torrent, alternative }: Props) {
  return (
    <div role="alert" className="absolute inset-0 grid place-items-center p-6">
      <div className="w-full max-w-[560px]">
        <div className="mb-[18px]">
          <ReleaseTag torrent={torrent} />
        </div>
        <h2 className="m-0 mb-2.5 text-[26px] leading-tight font-extrabold">
          El reproductor integrado no puede abrir{" "}
          {torrent.videoCodec === "x265" ? "x265 (HEVC)" : "este archivo"}
        </h2>
        <p className="m-0 mb-6 text-base text-text-2">
          La descarga sigue en curso. Ábrela en VLC desde el mismo stream local
          {alternative ? ` o cambia a la versión ${alternative.quality} x264, que sí se reproduce aquí` : ""}.
        </p>
        <ExternalPlayerActions movieId={movieId} infohash={torrent.infohash} alternative={alternative} />
        <Link
          to="/movie/$movieId"
          params={{ movieId }}
          className="mt-4 inline-block text-sm font-semibold text-muted hover:text-text"
        >
          Volver a la ficha
        </Link>
      </div>
    </div>
  );
}
