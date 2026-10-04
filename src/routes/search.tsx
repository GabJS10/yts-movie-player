import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { useInfiniteMovies } from "../api/queries";
import { ErrorState } from "../components/ErrorState";
import { Icon } from "../components/Icon";
import { MovieCard, MovieCardSkeleton } from "../components/MovieCard";
import { SearchFilters } from "../components/SearchFilters";
import { validateCatalogSearch, type CatalogSearch } from "../lib/searchParams";
import { useDebouncedValue } from "../lib/useDebouncedValue";
import { useSearchMemory } from "../store/searchMemory";
import { useInView } from "../lib/useInView";

export const Route = createFileRoute("/search")({
  validateSearch: validateCatalogSearch,
  component: SearchPage,
});

export const SEARCH_DEBOUNCE_MS = 300;
const nf = new Intl.NumberFormat("es-ES");

function SearchPage() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  const [text, setText] = useState(search.query ?? "");
  const debounced = useDebouncedValue(text, SEARCH_DEBOUNCE_MS);
  const lastPushed = useRef(search.query ?? "");
  const remember = useSearchMemory((s) => s.remember);
  useEffect(() => remember(search), [search, remember]);

  // Typing → URL (replace: one history entry per search, not per keystroke).
  useEffect(() => {
    // Only once typing has settled: a stale debounced value must not undo a clear or a back navigation.
    if (debounced !== text) return;
    const q = debounced.trim();
    if (q === (search.query ?? "")) return;
    lastPushed.current = q;
    void navigate({ search: (prev) => ({ ...prev, query: q || undefined }), replace: true });
  }, [debounced, text, navigate, search.query]);

  // URL → input (back/forward, links).
  useEffect(() => {
    const q = search.query ?? "";
    if (q !== lastPushed.current) {
      lastPushed.current = q;
      setText(q);
    }
  }, [search.query]);

  const setFilters = (patch: Partial<CatalogSearch>) =>
    void navigate({ search: (prev) => ({ ...prev, ...patch }) });

  const filtered = !!(search.genre || search.quality || search.minimumRating);
  const clearAll = () => {
    setText("");
    lastPushed.current = "";
    void navigate({ search: {} });
  };

  return (
    <div className="min-h-screen px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-24">
      <h1 className="sr-only">Buscar</h1>
      <div className="relative mb-4 max-w-[880px]">
        <Icon
          name="search"
          size={30}
          className="pointer-events-none absolute top-1/2 left-0 -translate-y-1/2 text-muted"
        />
        <input
          type="search"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Título, año o código IMDb"
          aria-label="Buscar películas"
          data-testid="search-input"
          autoComplete="off"
          autoFocus={!filtered}
          className="h-[72px] w-full border-0 border-b-2 border-line-hi bg-transparent pl-[46px] text-[clamp(28px,3vw,40px)] font-bold outline-none stretch-semi transition-colors focus:border-green"
        />
      </div>
      <SearchFilters value={search} onChange={setFilters} />
      <Results search={search} onClear={clearAll} canClear={filtered || !!search.query} />
    </div>
  );
}

function Results({
  search,
  onClear,
  canClear,
}: {
  search: CatalogSearch;
  onClear: () => void;
  canClear: boolean;
}) {
  const q = useInfiniteMovies({ ...search, limit: 30 });
  const [sentinel, nearEnd] = useInView<HTMLDivElement>({ rootMargin: "0px 0px 800px 0px" });
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = q;
  useEffect(() => {
    if (nearEnd && hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [nearEnd, hasNextPage, isFetchingNextPage, fetchNextPage]);

  if (q.isError && !q.data) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;

  if (q.isSuccess && q.data.movies.length === 0) {
    return (
      <div className="grid max-w-[460px] justify-items-start gap-3.5 py-16">
        <h2 className="m-0 text-[22px] font-extrabold">
          {search.query ? `Nada coincide con «${search.query}»` : "Ninguna película cumple estos filtros"}
        </h2>
        <p className="m-0 text-muted">
          Prueba con otro título, revisa la ortografía o quita algún filtro. También puedes buscar por año o
          por código IMDb (tt…).
        </p>
        {canClear && (
          <button type="button" className="btn btn-line btn-sm" onClick={onClear}>
            Quitar filtros
          </button>
        )}
      </div>
    );
  }

  return (
    <>
      <div className="mb-[18px] flex items-baseline gap-3.5">
        <h2 className="m-0 text-lg font-bold tnum" aria-live="polite">
          {q.isSuccess
            ? q.data.total === 1
              ? "1 película"
              : `${nf.format(q.data.total)} películas`
            : "Buscando…"}
        </h2>
        {canClear && (
          <button
            type="button"
            className="cursor-pointer border-0 bg-transparent p-0 text-sm font-semibold text-green hover:underline"
            onClick={onClear}
          >
            Quitar filtros
          </button>
        )}
      </div>
      <div
        className="grid-cards grid grid-cols-[repeat(auto-fill,minmax(clamp(140px,13vw,190px),1fr))] gap-x-2.5 gap-y-7"
        aria-busy={q.isFetching || undefined}
      >
        {q.data
          ? q.data.movies.map((m) => <MovieCard key={m.id} movie={m} />)
          : Array.from({ length: 12 }, (_, i) => <MovieCardSkeleton key={i} />)}
        {isFetchingNextPage && Array.from({ length: 6 }, (_, i) => <MovieCardSkeleton key={`n${i}`} />)}
      </div>
      {hasNextPage && <div ref={sentinel} className="h-px" aria-hidden="true" />}
    </>
  );
}
