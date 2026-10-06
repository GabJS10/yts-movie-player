import { createFileRoute, Link } from "@tanstack/react-router";
import { useDownloads, useFavorites, useHomeProfile } from "../api/queries";
import { ContinueRow } from "../components/ContinueRow";
import { HeroBanner } from "../components/HeroBanner";
import { MovieRow, StaticMovieRow } from "../components/MovieRow";
import { useT, type Messages } from "../i18n";
import type { GenreValue } from "../lib/genres";
import { orderRows, type RowDef } from "../lib/home";
import { useConnectivity } from "../store/connectivity";

const GENRE_ROWS: GenreValue[] = ["action", "comedy", "sci-fi", "horror", "animation", "drama"];

const homeRows = (t: Messages): RowDef[] => [
  { title: t.home.trending, note: t.home.trendingNote, params: { sortBy: "download_count" } },
  { title: t.home.newest, params: { sortBy: "date_added" } },
  { title: t.home.topRated, note: t.home.topRatedNote, params: { sortBy: "rating", minimumRating: 7 } },
  ...GENRE_ROWS.map((genre) => ({
    title: t.genres[genre],
    params: { genre, sortBy: "download_count" as const },
  })),
];

export const Route = createFileRoute("/")({ component: HomePage });

function HomePage() {
  const t = useT();
  const favorites = useFavorites();
  // Without network only what's stored locally stays: Continuar viendo, Mi lista and the library.
  const offline = useConnectivity((s) => s.offline);
  const profile = useHomeProfile().data;
  const because = profile?.becauseWatched;
  return (
    <>
      {offline ? <OfflineHeader /> : <HeroBanner />}
      <div className={`relative z-[2] pb-20 ${offline ? "" : "-mt-10 max-[520px]:-mt-6"}`}>
        {offline && <LibraryRow />}
        <ContinueRow />
        {!offline && because && because.movies.length > 0 && (
          <StaticMovieRow title={t.featured.becauseWatched(because.sourceTitle)} movies={because.movies} />
        )}
        {favorites.data && favorites.data.length > 0 && (
          <StaticMovieRow title={t.nav.myList} movies={favorites.data} more={{ to: "/my-list" }} />
        )}
        {!offline &&
          orderRows(homeRows(t), profile?.genreOrder ?? []).map((r) => (
            <MovieRow
              key={r.params.genre ?? r.params.sortBy}
              title={r.title}
              note={r.note}
              params={r.params}
              more={{
                to: "/search",
                search: {
                  sortBy: r.params.sortBy,
                  genre: r.params.genre,
                  minimumRating: r.params.minimumRating,
                },
              }}
            />
          ))}
      </div>
    </>
  );
}

function OfflineHeader() {
  const t = useT();
  return (
    <div className="px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-8">
      <h1 className="m-0 mb-1.5 text-headline font-[850] uppercase stretch-condensed">{t.nav.offline}</h1>
      <p className="m-0 max-w-[60ch] text-[15px] text-muted">{t.home.offlineBody}</p>
    </div>
  );
}

/** Finished downloads, to watch without network. */
function LibraryRow() {
  const downloads = useDownloads();
  const t = useT();
  const movies = (downloads.data ?? []).filter((d) => d.state === "done").map((d) => d.movie);
  if (downloads.isSuccess && movies.length === 0) {
    return (
      <p className="mb-[34px] px-gutter text-muted">
        {t.home.noDownloads}{" "}
        <Link to="/downloads" className="font-semibold text-green hover:underline">
          {t.home.goToDownloads}
        </Link>
      </p>
    );
  }
  return (
    <StaticMovieRow
      title={t.home.library}
      movies={downloads.data ? movies : undefined}
      loading={downloads.isPending}
      more={{ to: "/downloads" }}
    />
  );
}
