import type { MovieSummary } from "../api/types";
import { useLanguage } from "../i18n";
import { formatRating, formatRuntime } from "../lib/format";
import { genreLabel } from "../lib/genres";
import { Icon } from "./Icon";

type Props = { movie: MovieSummary; mpaRating?: string | null; genres?: number; className?: string };

const Sep = () => <span aria-hidden="true" className="size-[3px] rounded-full bg-faint" />;

/** ★ 8,7 IMDb · 2014 · 2 h 49 min · PG-13 · Acción · Aventura */
export function MovieMeta({ movie, mpaRating, genres = 0, className = "" }: Props) {
  useLanguage(); // genre labels and number formats follow the UI language
  return (
    <div
      className={`flex flex-wrap items-center gap-x-3.5 gap-y-1.5 text-[15px] text-text-2 tnum ${className}`}
    >
      <span className="inline-flex items-center gap-1.5 font-bold text-text">
        <Icon name="star" size={16} className="flex-none text-green" />
        {formatRating(movie.rating)}
        <small className="text-[11px] font-semibold tracking-[0.06em] text-muted">IMDb</small>
      </span>
      <span>{movie.year}</span>
      {movie.runtimeMin > 0 && (
        <>
          <Sep />
          <span>{formatRuntime(movie.runtimeMin)}</span>
        </>
      )}
      {mpaRating && (
        <span className="rounded-sm border border-line-hi px-1.5 text-xs leading-5">{mpaRating}</span>
      )}
      {genres > 0 && movie.genres.length > 0 && (
        <>
          <Sep />
          <span>{movie.genres.slice(0, genres).map(genreLabel).join(" · ")}</span>
        </>
      )}
    </div>
  );
}
