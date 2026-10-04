import { createFileRoute, Link } from "@tanstack/react-router";
import { useState } from "react";
import { useFavorites } from "../api/queries";
import { ErrorState } from "../components/ErrorState";
import { MovieCard, MovieCardSkeleton } from "../components/MovieCard";
import { sortFavorites, type FavoriteSort } from "../lib/movie";

export const Route = createFileRoute("/my-list")({ component: MyListPage });

const SORTS: { value: FavoriteSort; label: string }[] = [
  { value: "added", label: "Añadidas" },
  { value: "rating", label: "Valoración" },
  { value: "year", label: "Año" },
  { value: "title", label: "Título" },
];

const grid =
  "grid-cards grid grid-cols-[repeat(auto-fill,minmax(clamp(140px,13vw,190px),1fr))] gap-x-2.5 gap-y-7";

function MyListPage() {
  const favorites = useFavorites();
  const [sort, setSort] = useState<FavoriteSort>("added");
  const movies = favorites.data;

  return (
    <div className="min-h-screen px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-24">
      <h1 className="m-0 mb-1.5 text-headline font-[850] uppercase stretch-condensed">Mi lista</h1>

      {favorites.isError ? (
        <div className="mt-8">
          <ErrorState error={favorites.error} onRetry={() => void favorites.refetch()} />
        </div>
      ) : !movies ? (
        <div className={`${grid} mt-8`} aria-busy="true" aria-label="Cargando Mi lista">
          {Array.from({ length: 6 }, (_, i) => (
            <MovieCardSkeleton key={i} />
          ))}
        </div>
      ) : movies.length === 0 ? (
        <div className="grid max-w-[460px] justify-items-start gap-3.5 py-16">
          <h2 className="m-0 text-[22px] font-extrabold">Tu lista está vacía</h2>
          <p className="m-0 text-muted">
            Pulsa «Mi lista» en la ficha de cualquier película y aparecerá aquí y en el inicio.
          </p>
          <Link to="/" className="btn btn-line btn-sm">
            Explorar el catálogo
          </Link>
        </div>
      ) : (
        <>
          <p className="m-0 mb-8 text-[15px] text-muted tnum">
            {movies.length === 1 ? "1 película guardada" : `${movies.length} películas guardadas`} para
            después
          </p>
          <div className="mb-6 flex items-end border-b border-line pt-1.5 pb-5">
            <div className="grid gap-1.5">
              <span className="field-label" id="mylist-sort">
                Ordenar
              </span>
              <div className="seg" role="group" aria-labelledby="mylist-sort">
                {SORTS.map((s) => (
                  <button
                    key={s.value}
                    type="button"
                    aria-pressed={sort === s.value}
                    onClick={() => setSort(s.value)}
                  >
                    {s.label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <div className={grid}>
            {sortFavorites(movies, sort).map((m) => (
              <MovieCard key={m.id} movie={m} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
