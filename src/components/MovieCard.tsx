import { Link } from "@tanstack/react-router";
import { useFavorites } from "../api/queries";
import type { MovieSummary, Quality } from "../api/types";
import { useT } from "../i18n";
import { formatRating, formatRuntime } from "../lib/format";
import { qualityRank } from "../lib/versions";
import { Icon } from "./Icon";
import { Poster } from "./Poster";
import { SwarmSignal } from "./SwarmSignal";

/** Poster card. Hover/focus reveals title, meta and available qualities. */
export function MovieCard({ movie }: { movie: MovieSummary }) {
  const t = useT();
  const order = (q: Quality) => (q === "3D" ? 99 : qualityRank(q));
  const qualities = [...movie.qualities].sort((a, b) => order(a) - order(b));
  // Shared, cached list: every card reads the same query.
  const inList = useFavorites().data?.some((f) => f.id === movie.id) ?? false;
  return (
    <Link
      to="/movie/$movieId"
      params={{ movieId: movie.id }}
      className="card"
      aria-label={`${movie.title} (${movie.year})`}
      data-card
      data-testid="movie-card"
      data-movie-id={movie.id}
    >
      <Poster src={movie.coverUrl} title={movie.title} />
      {inList && (
        <span
          className="absolute top-2 right-2 grid size-[22px] place-items-center rounded-full bg-[rgba(12,12,12,.75)] text-green"
          title={t.favorite.inList}
          aria-hidden="true"
          data-testid="card-in-list"
        >
          <Icon name="heart-fill" size={13} />
        </span>
      )}
      <div className="card-info" aria-hidden="true">
        <h3 className="m-0 mb-1 line-clamp-2 text-sm leading-tight font-[750]">{movie.title}</h3>
        <div className="mb-2 flex items-center gap-2 text-xs text-text-2 tnum">
          <span className="inline-flex items-center gap-[3px] font-bold text-text">
            <Icon name="star" size={12} className="flex-none text-green" />
            {formatRating(movie.rating)}
          </span>
          <span>{movie.year}</span>
          {movie.runtimeMin > 0 && <span>{formatRuntime(movie.runtimeMin)}</span>}
        </div>
        <div className="mb-2 flex flex-wrap gap-1">
          {qualities.map((q) => (
            <span key={q} className={`qchip ${movie.hasX264 ? "" : "qchip-hevc"}`}>
              {q}
            </span>
          ))}
        </div>
        <SwarmSignal seeds={movie.maxSeeds} />
      </div>
    </Link>
  );
}

export function MovieCardSkeleton() {
  return <div className="card skeleton" aria-hidden="true" />;
}
