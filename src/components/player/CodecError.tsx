import { Link } from "@tanstack/react-router";
import type { ExternalSubtitleArgs, Torrent } from "../../api/types";
import { useT } from "../../i18n";
import { ReleaseTag } from "../ReleaseTag";
import { ExternalPlayerActions } from "./ExternalPlayerActions";

type Props = {
  movieId: number;
  torrent: Torrent;
  alternative: Torrent | null;
  subtitles?: ExternalSubtitleArgs;
};

/** The WebView can't decode this file (typically x265/HEVC): offer VLC and an x264 version. */
export function CodecError({ movieId, torrent, alternative, subtitles }: Props) {
  const t = useT().codec;
  return (
    <div role="alert" className="absolute inset-0 grid place-items-center p-6">
      <div className="w-full max-w-[560px]">
        <div className="mb-[18px]">
          <ReleaseTag torrent={torrent} />
        </div>
        <h2 className="m-0 mb-2.5 text-[26px] leading-tight font-extrabold">
          {torrent.videoCodec === "x265" ? t.titleHevc : t.titleFile}
        </h2>
        <p className="m-0 mb-6 text-base text-text-2">
          {alternative ? t.bodyWithAlternative(alternative.quality) : t.body}
        </p>
        <ExternalPlayerActions
          movieId={movieId}
          infohash={torrent.infohash}
          alternative={alternative}
          subtitles={subtitles}
        />
        <Link
          to="/movie/$movieId"
          params={{ movieId }}
          className="mt-4 inline-block text-sm font-semibold text-muted hover:text-text"
        >
          {t.backToMovie}
        </Link>
      </div>
    </div>
  );
}
