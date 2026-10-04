import { createFileRoute, Link } from "@tanstack/react-router";
import { useDownloads, useFavorites, useHomeProfile } from "../api/queries";
import { ContinueRow } from "../components/ContinueRow";
import { HeroBanner } from "../components/HeroBanner";
import { MovieRow, StaticMovieRow } from "../components/MovieRow";
import { orderRows, type RowDef } from "../lib/home";
import { useConnectivity } from "../store/connectivity";

const ROWS: RowDef[] = [
  { title: "Tendencias en YTS", note: "Más descargadas", params: { sortBy: "download_count" } },
  { title: "Recién llegadas", params: { sortBy: "date_added" } },
  { title: "Mejor valoradas", note: "IMDb 7 o más", params: { sortBy: "rating", minimumRating: 7 } },
  { title: "Acción", params: { genre: "action", sortBy: "download_count" } },
  { title: "Comedia", params: { genre: "comedy", sortBy: "download_count" } },
  { title: "Ciencia ficción", params: { genre: "sci-fi", sortBy: "download_count" } },
  { title: "Terror", params: { genre: "horror", sortBy: "download_count" } },
  { title: "Animación", params: { genre: "animation", sortBy: "download_count" } },
  { title: "Drama", params: { genre: "drama", sortBy: "download_count" } },
];

export const Route = createFileRoute("/")({ component: HomePage });

function HomePage() {
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
          <StaticMovieRow title={`Porque viste ${because.sourceTitle}`} movies={because.movies} />
        )}
        {favorites.data && favorites.data.length > 0 && (
          <StaticMovieRow title="Mi lista" movies={favorites.data} more={{ to: "/my-list" }} />
        )}
        {!offline &&
          orderRows(ROWS, profile?.genreOrder ?? []).map((r) => (
            <MovieRow
              key={r.title}
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
  return (
    <div className="px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-8">
      <h1 className="m-0 mb-1.5 text-headline font-[850] uppercase stretch-condensed">Sin conexión</h1>
      <p className="m-0 max-w-[60ch] text-[15px] text-muted">
        El catálogo de YTS no responde. Lo que descargaste se ve igual, desde tu biblioteca; el resto vuelve
        solo cuando haya conexión.
      </p>
    </div>
  );
}

/** Finished downloads, to watch without network. */
function LibraryRow() {
  const downloads = useDownloads();
  const movies = (downloads.data ?? []).filter((d) => d.state === "done").map((d) => d.movie);
  if (downloads.isSuccess && movies.length === 0) {
    return (
      <p className="mb-[34px] px-gutter text-muted">
        Aún no tienes películas descargadas.{" "}
        <Link to="/downloads" className="font-semibold text-green hover:underline">
          Ir a Descargas
        </Link>
      </p>
    );
  }
  return (
    <StaticMovieRow
      title="En tu biblioteca"
      movies={downloads.data ? movies : undefined}
      loading={downloads.isPending}
      more={{ to: "/downloads" }}
    />
  );
}
