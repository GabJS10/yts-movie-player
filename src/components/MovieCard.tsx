import { Link } from "@tanstack/react-router";
import type { MovieSummary, Quality } from "../api/types";
import { formatRating, formatRuntime } from "../lib/format";
import { qualityRank } from "../lib/versions";
import { Icon } from "./Icon";

/** Poster card. Hover/focus reveals title, meta and available qualities. */
export function MovieCard({ movie }: { movie: MovieSummary }) {
  const order = (q: Quality) => (q === "3D" ? 99 : qualityRank(q));
  const qualities = [...movie.qualities].sort((a, b) => order(a) - order(b));
  return (
    <Link
      to="/movie/$movieId"
      params={{ movieId: movie.id }}
      className="card"
      aria-label={`${movie.title} (${movie.year})`}
      data-card
    >
      <img src={movie.coverUrl} alt="" loading="lazy" decoding="async" width={230} height={345} />
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
        <div className="flex flex-wrap gap-1">
          {qualities.map((q) => (
            <span key={q} className={`qchip ${movie.hasX264 ? "" : "qchip-hevc"}`}>
              {q}
            </span>
          ))}
        </div>
      </div>
    </Link>
  );
}

export function MovieCardSkeleton() {
  return <div className="card skeleton" aria-hidden="true" />;
}
