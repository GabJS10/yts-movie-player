import { Link, type LinkProps } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useInfiniteMovies } from "../api/queries";
import type { ListMoviesParams, MovieSummary } from "../api/types";
import { onRowKeyDown } from "../lib/rowNav";
import { useInView } from "../lib/useInView";
import { ErrorState } from "./ErrorState";
import { Icon } from "./Icon";
import { MovieCard, MovieCardSkeleton } from "./MovieCard";

type RowShellProps = {
  title: string;
  note?: string;
  more?: LinkProps;
  children: ReactNode;
  trackRef?: (el: HTMLDivElement | null) => void;
  busy?: boolean;
};

const SKELETONS = Array.from({ length: 8 }, (_, i) => <MovieCardSkeleton key={i} />);

export function RowShell({ title, note, more, children, trackRef, busy }: RowShellProps) {
  const track = useRef<HTMLDivElement | null>(null);
  const [edges, setEdges] = useState({ start: true, end: false });
  const sync = useCallback(() => {
    const el = track.current;
    if (!el) return;
    setEdges({
      start: el.scrollLeft < 8,
      end: el.scrollLeft + el.clientWidth >= el.scrollWidth - 8,
    });
  }, []);
  useEffect(sync, [sync, children]);

  const page = (dir: 1 | -1) => {
    const el = track.current;
    if (!el) return;
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    el.scrollBy({ left: dir * el.clientWidth * 0.85, behavior: reduce ? "auto" : "smooth" });
  };
  const titleId = `row-${title.replace(/\W+/g, "-").toLowerCase()}`;

  return (
    <section className="group/row mb-[34px]" aria-labelledby={titleId} aria-busy={busy || undefined}>
      <div className="mb-1 flex items-baseline gap-4 px-gutter">
        <h2 id={titleId} className="m-0 text-title font-[750]">
          {title}
        </h2>
        {note && <span className="text-[13px] text-muted max-[900px]:hidden">{note}</span>}
        {more && (
          <Link
            {...more}
            className="inline-flex -translate-x-1.5 items-center gap-0.5 text-[13px] font-semibold text-green opacity-0 transition group-hover/row:translate-x-0 group-hover/row:opacity-100 focus-visible:translate-x-0 focus-visible:opacity-100"
          >
            Ver todo
            <Icon name="chev-r" size={14} />
          </Link>
        )}
      </div>
      <div className="row-viewport relative">
        {!edges.start && (
          <button
            type="button"
            className="row-arrow left-0 rounded-r-md"
            aria-label="Anteriores"
            onClick={() => page(-1)}
          >
            <Icon name="chev-l" size={30} />
          </button>
        )}
        <div
          ref={(el) => {
            track.current = el;
            trackRef?.(el);
          }}
          className="row-track"
          data-row-track
          onScroll={sync}
          onKeyDown={onRowKeyDown}
        >
          {children}
        </div>
        {!edges.end && (
          <button
            type="button"
            className="row-arrow right-0 rounded-l-md"
            aria-label="Siguientes"
            onClick={() => page(1)}
          >
            <Icon name="chev-r" size={30} />
          </button>
        )}
      </div>
    </section>
  );
}

type MovieRowProps = {
  title: string;
  params: Omit<ListMoviesParams, "page">;
  note?: string;
  more?: LinkProps;
};

/** Catalog row with horizontal scroll; reaching the end loads the next API page. */
export function MovieRow({ title, params, note, more }: MovieRowProps) {
  const q = useInfiniteMovies(params);
  const [trackEl, setTrackEl] = useState<HTMLDivElement | null>(null);
  const [sentinel, nearEnd] = useInView<HTMLDivElement>({ root: trackEl, rootMargin: "0px 800px 0px 0px" });
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = q;

  useEffect(() => {
    if (nearEnd && hasNextPage && !isFetchingNextPage) void fetchNextPage();
  }, [nearEnd, hasNextPage, isFetchingNextPage, fetchNextPage]);

  if (q.isError && !q.data) {
    return (
      <section className="mb-[34px] px-gutter" aria-label={title}>
        <h2 className="m-0 text-title font-[750]">{title}</h2>
        <ErrorState error={q.error} onRetry={() => void q.refetch()} compact />
      </section>
    );
  }
  if (q.isSuccess && q.data.movies.length === 0) return null;

  return (
    <RowShell title={title} note={note} more={more} trackRef={setTrackEl} busy={q.isFetching}>
      {q.data ? q.data.movies.map((m) => <MovieCard key={m.id} movie={m} />) : SKELETONS}
      {isFetchingNextPage && SKELETONS.slice(0, 3)}
      {hasNextPage && <div ref={sentinel} className="w-px flex-none" aria-hidden="true" />}
    </RowShell>
  );
}

/** Row over a fixed list (e.g. Similares). */
export function StaticMovieRow({
  title,
  movies,
  loading,
  more,
}: {
  title: string;
  movies: MovieSummary[] | undefined;
  loading?: boolean;
  more?: LinkProps;
}) {
  if (!loading && (!movies || movies.length === 0)) return null;
  return (
    <RowShell title={title} busy={loading} more={more}>
      {movies ? movies.map((m) => <MovieCard key={m.id} movie={m} />) : SKELETONS.slice(0, 4)}
    </RowShell>
  );
}
