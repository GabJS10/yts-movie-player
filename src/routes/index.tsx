import { createFileRoute } from "@tanstack/react-router";
import { useFavorites } from "../api/queries";
import { ContinueRow } from "../components/ContinueRow";
import { HeroBanner } from "../components/HeroBanner";
import { MovieRow, StaticMovieRow } from "../components/MovieRow";
import type { ListMoviesParams } from "../api/types";

type RowDef = { title: string; params: Omit<ListMoviesParams, "page">; note?: string };

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
  return (
    <>
      <HeroBanner />
      <div className="relative z-[2] -mt-10 pb-20 max-[520px]:-mt-6">
        <ContinueRow />
        {favorites.data && favorites.data.length > 0 && (
          <StaticMovieRow title="Mi lista" movies={favorites.data} more={{ to: "/my-list" }} />
        )}
        {ROWS.map((r) => (
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
