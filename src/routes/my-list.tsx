import { createFileRoute, Link } from "@tanstack/react-router";
import { useState } from "react";
import { useFavorites } from "../api/queries";
import { ErrorState } from "../components/ErrorState";
import { MovieCard, MovieCardSkeleton } from "../components/MovieCard";
import { useT } from "../i18n";
import { sortFavorites, type FavoriteSort } from "../lib/movie";

export const Route = createFileRoute("/my-list")({ component: MyListPage });

const SORTS: FavoriteSort[] = ["added", "rating", "year", "title"];

const grid =
  "grid-cards grid grid-cols-[repeat(auto-fill,minmax(clamp(140px,13vw,190px),1fr))] gap-x-2.5 gap-y-7";

function MyListPage() {
  const favorites = useFavorites();
  const t = useT();
  const [sort, setSort] = useState<FavoriteSort>("added");
  const movies = favorites.data;

  return (
    <div className="min-h-screen px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-24">
      <h1 className="m-0 mb-1.5 text-headline font-[850] uppercase stretch-condensed">{t.nav.myList}</h1>

      {favorites.isError ? (
        <div className="mt-8">
          <ErrorState error={favorites.error} onRetry={() => void favorites.refetch()} />
        </div>
      ) : !movies ? (
        <div className={`${grid} mt-8`} aria-busy="true" aria-label={t.myList.loading}>
          {Array.from({ length: 6 }, (_, i) => (
            <MovieCardSkeleton key={i} />
          ))}
        </div>
      ) : movies.length === 0 ? (
        <div className="grid max-w-[460px] justify-items-start gap-3.5 py-16">
          <h2 className="m-0 text-[22px] font-extrabold">{t.myList.emptyTitle}</h2>
          <p className="m-0 text-muted">{t.myList.emptyBody}</p>
          <Link to="/" className="btn btn-line btn-sm">
            {t.myList.explore}
          </Link>
        </div>
      ) : (
        <>
          <p className="m-0 mb-8 text-[15px] text-muted tnum">{t.myList.count(movies.length)}</p>
          <div className="mb-6 flex items-end border-b border-line pt-1.5 pb-5">
            <div className="grid gap-1.5">
              <span className="field-label" id="mylist-sort">
                {t.myList.sort}
              </span>
              <div className="seg" role="group" aria-labelledby="mylist-sort">
                {SORTS.map((s) => (
                  <button key={s} type="button" aria-pressed={sort === s} onClick={() => setSort(s)}>
                    {t.myList.sorts[s]}
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
