import { Link } from "@tanstack/react-router";
import { useHeroMovie, useMovie } from "../api/queries";
import { pickDefaultTorrent } from "../lib/versions";
import { ErrorState } from "./ErrorState";
import { Icon } from "./Icon";
import { MovieMeta } from "./MovieMeta";
import { ReleaseTag } from "./ReleaseTag";
import { SwarmSignal } from "./SwarmSignal";

const heroFrame = "relative h-[min(88vh,900px)] min-h-[560px] overflow-hidden";
const scrim =
  "pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,rgba(23,23,23,.92)_0%,rgba(23,23,23,.72)_28%,rgba(23,23,23,0)_62%),linear-gradient(0deg,var(--color-ground)_0%,rgba(23,23,23,0)_34%)]";
const bodyPos =
  "absolute bottom-[clamp(84px,10vh,110px)] left-gutter z-[1] w-[min(620px,calc(100%-2*var(--spacing-gutter)))] max-[520px]:bottom-16";

/** Home banner: most-downloaded movie with artwork; release facts of its default version. */
export function HeroBanner() {
  const hero = useHeroMovie();
  const movie = hero.data;
  // The summary has no torrents/synopsis: the detail query fills them in (and warms the movie page).
  const detail = useMovie(movie?.id ?? 0, { enabled: !!movie });
  const best = detail.data ? pickDefaultTorrent(detail.data.torrents) : null;

  if (hero.isError) {
    return (
      <section className={`${heroFrame} grid items-end px-gutter pb-28`} aria-label="Destacada">
        <ErrorState error={hero.error} onRetry={() => void hero.refetch()} />
      </section>
    );
  }
  if (hero.isSuccess && !movie) return <div className="h-[calc(var(--spacing-nav)+24px)]" />;

  return (
    <section className={heroFrame} aria-label="Destacada" aria-busy={hero.isPending || undefined}>
      {movie?.backgroundUrl ? (
        <img
          src={movie.backgroundUrl}
          alt=""
          className="absolute inset-0 size-full object-cover object-[60%_30%]"
          decoding="async"
        />
      ) : (
        <div className="skeleton absolute inset-0 rounded-none" aria-hidden="true" />
      )}
      <div className={scrim} />
      <div className={bodyPos}>
        {movie ? (
          <>
            <h1 className="m-0 mb-3.5 text-display font-[850] text-balance uppercase stretch-condensed">
              {movie.title}
            </h1>
            <MovieMeta movie={movie} mpaRating={detail.data?.mpaRating} genres={3} />
            <p className="mt-3 mb-4 line-clamp-2 min-h-[2lh] max-w-[54ch] text-[17px] leading-normal text-text-2 [text-shadow:0_1px_12px_rgba(0,0,0,.5)]">
              {detail.data?.summary ?? " "}
            </p>
            <div className="mb-5 flex min-h-6 flex-wrap items-center gap-x-4 gap-y-3">
              {best && (
                <>
                  <ReleaseTag torrent={best} on />
                  <SwarmSignal seeds={best.seeds} peers={best.peers} />
                </>
              )}
            </div>
            <div className="flex flex-wrap gap-3">
              <button type="button" className="btn btn-play" disabled title="Llega con la fase de streaming">
                <Icon name="play" size={22} />
                Reproducir
              </button>
              <Link to="/movie/$movieId" params={{ movieId: movie.id }} className="btn btn-ghost">
                <Icon name="info" size={22} />
                Más info
              </Link>
            </div>
          </>
        ) : (
          <div aria-hidden="true">
            <div className="skeleton mb-4 h-20 w-[70%]" />
            <div className="skeleton mb-4 h-5 w-[55%]" />
            <div className="skeleton mb-6 h-12 w-[90%]" />
            <div className="flex gap-3">
              <div className="skeleton h-12 w-40" />
              <div className="skeleton h-12 w-36" />
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
