import { createFileRoute, Link, notFound } from "@tanstack/react-router";
import { useMovie, useSettings, useTorrentPrefs } from "../api/queries";
import { ErrorState } from "../components/ErrorState";
import { Player } from "../components/player/Player";
import { useT } from "../i18n";
import { validatePlaySearch } from "../lib/searchParams";
import { pickDefaultTorrent } from "../lib/versions";

export const Route = createFileRoute("/play/$movieId")({
  params: {
    parse: ({ movieId }) => {
      const id = Number(movieId);
      if (!Number.isInteger(id) || id <= 0) throw notFound();
      return { movieId: id };
    },
    stringify: ({ movieId }) => ({ movieId: String(movieId) }),
  },
  validateSearch: validatePlaySearch,
  component: PlayerPage,
});

// Full-screen surface: the shell hides its navigation on /play.
function PlayerPage() {
  const { movieId } = Route.useParams();
  const { infohash, from } = Route.useSearch();
  const movie = useMovie(movieId);
  const prefs = useTorrentPrefs();
  const t = useT().play;
  // Without a chosen version the default depends on the settings: wait for them (or their failure),
  // or the player would start one stream and switch to another when they arrive.
  const settingsPending = useSettings().isPending && !infohash;

  if (movie.isError) {
    return (
      <div className="fixed inset-0 z-[100] grid place-items-center bg-black p-6">
        <ErrorState error={movie.error} onRetry={() => void movie.refetch()} />
      </div>
    );
  }
  if (!movie.data || settingsPending) {
    return <div className="fixed inset-0 z-[100] bg-black" aria-busy="true" aria-label={t.loading} />;
  }

  const torrent =
    movie.data.torrents.find((tr) => tr.infohash === infohash) ??
    pickDefaultTorrent(movie.data.torrents, prefs);
  if (!torrent) {
    return (
      <div className="fixed inset-0 z-[100] grid place-items-center bg-black p-6">
        <div className="grid max-w-[520px] justify-items-start gap-3">
          <h2 className="m-0 text-[22px] font-extrabold">{t.noVersions}</h2>
          <Link to="/movie/$movieId" params={{ movieId }} className="btn btn-line btn-sm">
            {t.backToMovie}
          </Link>
        </div>
      </div>
    );
  }
  // key: switching version (e.g. "Cambiar a 1080p x264") restarts the whole session.
  return <Player key={torrent.infohash} movie={movie.data} torrent={torrent} fromStart={from === "start"} />;
}
