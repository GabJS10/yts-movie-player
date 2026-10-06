import { Link } from "@tanstack/react-router";
import { useEffect, useState, type FocusEvent, type KeyboardEvent, type ReactNode } from "react";
import { useFeatured, useHeroMovie, useMovie, useTorrentPrefs } from "../api/queries";
import type { FeaturedItem, MovieDetail, MovieSummary } from "../api/types";
import { useT } from "../i18n";
import { heroArt, reasonLabel, ROTATE_MS } from "../lib/featured";
import { usePageHidden, useReducedMotion } from "../lib/motion";
import { pickDefaultTorrent } from "../lib/versions";
import { ErrorState } from "./ErrorState";
import { FavoriteButton } from "./FavoriteButton";
import { Icon } from "./Icon";
import { MovieMeta } from "./MovieMeta";
import { ReleaseTag } from "./ReleaseTag";
import { SwarmSignal } from "./SwarmSignal";

const heroFrame = "relative h-[min(88vh,900px)] min-h-[560px] overflow-hidden";
const scrim =
  "pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,rgba(23,23,23,.92)_0%,rgba(23,23,23,.72)_28%,rgba(23,23,23,0)_62%),linear-gradient(0deg,var(--color-ground)_0%,rgba(23,23,23,0)_34%)]";
const bodyPos =
  "absolute bottom-[clamp(84px,10vh,110px)] left-gutter z-[1] w-[min(620px,calc(100%-2*var(--spacing-gutter)))] max-[520px]:bottom-16";
const artClass = "absolute inset-0 size-full object-cover object-[60%_30%]";

/**
 * Home banner. With recommendations (get_featured), a rotating banner with the reason for each one;
 * without them (no history and no network, or an error), the most downloaded movie as before.
 */
export function HeroBanner() {
  const featured = useFeatured();
  if (featured.isPending) return <HeroSkeleton />;
  if (featured.data && featured.data.length > 0) return <HeroCarousel items={featured.data} />;
  return <SingleHero />;
}

function HeroSkeleton() {
  const t = useT();
  return (
    <section className={heroFrame} aria-label={t.hero.label} aria-busy="true" data-testid="hero">
      <div className="skeleton absolute inset-0 rounded-none" aria-hidden="true" />
      <div className={scrim} />
      <div className={bodyPos}>
        <BodySkeleton />
      </div>
    </section>
  );
}

function BodySkeleton() {
  return (
    <div aria-hidden="true">
      <div className="skeleton mb-4 h-20 w-[70%]" />
      <div className="skeleton mb-4 h-5 w-[55%]" />
      <div className="skeleton mb-6 h-12 w-[90%]" />
      <div className="flex gap-3">
        <div className="skeleton h-12 w-40" />
        <div className="skeleton h-12 w-36" />
      </div>
    </div>
  );
}

/** Title, meta, synopsis, default version and actions of one movie. */
function HeroBody({
  movie,
  detail,
  reason,
}: {
  movie: MovieSummary;
  detail: MovieDetail | undefined;
  reason?: ReactNode;
}) {
  const prefs = useTorrentPrefs();
  const t = useT();
  const best = detail ? pickDefaultTorrent(detail.torrents, prefs) : null;
  return (
    <>
      {reason && (
        <p
          className="m-0 mb-2.5 text-[13px] font-bold tracking-[0.08em] text-green uppercase"
          data-testid="hero-reason"
        >
          {reason}
        </p>
      )}
      <h1
        className="m-0 mb-3.5 text-display font-[850] text-balance uppercase stretch-condensed"
        data-testid="hero-title"
      >
        {movie.title}
      </h1>
      <MovieMeta movie={movie} mpaRating={detail?.mpaRating} genres={3} />
      <p className="mt-3 mb-4 line-clamp-2 min-h-[2lh] max-w-[54ch] text-[17px] leading-normal text-text-2 [text-shadow:0_1px_12px_rgba(0,0,0,.5)]">
        {detail?.summary ?? " "}
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
        <Link
          to="/play/$movieId"
          params={{ movieId: movie.id }}
          search={best ? { infohash: best.infohash } : {}}
          className="btn btn-play"
          data-testid="hero-play"
        >
          <Icon name="play" size={22} />
          {t.hero.play}
        </Link>
        <Link
          to="/movie/$movieId"
          params={{ movieId: movie.id }}
          className="btn btn-ghost"
          data-testid="hero-info"
        >
          <Icon name="info" size={22} />
          {t.hero.moreInfo}
        </Link>
        <FavoriteButton
          movie={movie}
          isFavorite={detail?.isFavorite ?? null}
          variant="round"
          testId="hero-favorite"
        />
      </div>
    </>
  );
}

/** The fallback: most downloaded movie with artwork; stills and release facts come from get_movie. */
function SingleHero() {
  const hero = useHeroMovie();
  const t = useT();
  const movie = hero.data;
  // The summary has no stills, torrents or synopsis: the detail query fills them in (and warms the movie page).
  const detail = useMovie(movie?.id ?? 0, { enabled: !!movie });
  // Sharp still from the detail; the summary's backgroundUrl (small, blurred) only if the detail has none or fails.
  const art = detail.data ? heroArt(detail.data) : detail.isError ? (movie?.backgroundUrl ?? null) : null;

  if (hero.isError) {
    return (
      <section
        className={`${heroFrame} grid items-end px-gutter pb-28`}
        aria-label={t.hero.label}
        data-testid="hero"
      >
        <ErrorState error={hero.error} onRetry={() => void hero.refetch()} />
      </section>
    );
  }
  if (hero.isSuccess && !movie) return <div className="h-[calc(var(--spacing-nav)+24px)]" />;

  return (
    <section
      className={heroFrame}
      aria-label={t.hero.label}
      aria-busy={hero.isPending || undefined}
      data-testid="hero"
    >
      {art ? (
        <img src={art} alt="" className={artClass} decoding="async" />
      ) : (
        <div className="skeleton absolute inset-0 rounded-none" aria-hidden="true" />
      )}
      <div className={scrim} />
      <div className={bodyPos}>
        {movie ? <HeroBody movie={movie} detail={detail.data} /> : <BodySkeleton />}
      </div>
    </section>
  );
}

const ctrl =
  "inline-grid size-10 place-items-center rounded-full bg-black/35 text-text-2 backdrop-blur-[8px] transition-colors hover:bg-black/55 hover:text-text";

/**
 * Rotating banner: advances every 8 s with a crossfade, preloading the next still. It stops while the
 * pointer or focus is on it, while the window is hidden, when the user pauses it, and with
 * prefers-reduced-motion. ←/→ inside it, the arrows and the dots move by hand.
 */
function HeroCarousel({ items }: { items: FeaturedItem[] }) {
  const n = items.length;
  const t = useT();
  const [index, setIndex] = useState(0);
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  const [paused, setPaused] = useState(false);
  const reduced = useReducedMotion();
  const hidden = usePageHidden();
  const rotating = n > 1 && !reduced && !paused && !hovered && !focused && !hidden;
  // Stills already requested: the current one and the next (preloaded) keep their src.
  const [loaded, setLoaded] = useState(() => new Set([0, 1 % n]));

  const go = (i: number) => {
    const next = ((i % n) + n) % n;
    setIndex(next);
    setLoaded((prev) =>
      prev.has(next) && prev.has((next + 1) % n) ? prev : new Set([...prev, next, (next + 1) % n]),
    );
  };

  useEffect(() => {
    if (!rotating) return;
    const timer = window.setTimeout(() => go(index + 1), ROTATE_MS);
    return () => window.clearTimeout(timer);
    // go is stable enough: it only reads n and the setters.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rotating, index, n]);

  const item = items[index] ?? items[0]!;
  const onKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    if ((e.target as HTMLElement).closest("input, textarea, select")) return;
    e.preventDefault();
    go(index + (e.key === "ArrowRight" ? 1 : -1));
  };
  const onBlur = (e: FocusEvent<HTMLElement>) => {
    if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
  };

  return (
    <section
      className={heroFrame}
      aria-label={t.hero.label}
      aria-roledescription={t.hero.carousel}
      data-testid="hero"
      data-index={index}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={onBlur}
      onKeyDown={onKeyDown}
    >
      {items.map((it, i) => {
        const art = heroArt(it.movie);
        return art && loaded.has(i) ? (
          <img
            key={it.movie.id}
            src={art}
            alt=""
            aria-hidden="true"
            decoding="async"
            className={`${artClass} transition-opacity duration-700 motion-reduce:transition-none ${i === index ? "opacity-100" : "opacity-0"}`}
          />
        ) : null;
      })}
      <div className={scrim} />
      <div
        className={bodyPos}
        role="group"
        aria-roledescription={t.hero.slide}
        aria-label={t.hero.position(index + 1, n, item.movie.title)}
      >
        <div key={item.movie.id} className="hero-in">
          <HeroBody movie={item.movie} detail={item.movie} reason={reasonLabel(item.reason)} />
        </div>
      </div>
      {/* Announced only when the user moves it (not on every auto-advance). */}
      <p className="sr-only" aria-live={rotating ? "off" : "polite"}>
        {t.hero.slidePosition(index + 1, n, item.movie.title)}
      </p>

      {n > 1 && (
        <div className="absolute right-gutter bottom-[clamp(84px,10vh,110px)] z-[2] flex items-center gap-2 max-[520px]:bottom-4">
          <button
            type="button"
            className={ctrl}
            aria-label={paused ? t.hero.resume : t.hero.pause}
            aria-pressed={paused}
            data-testid="hero-pause"
            hidden={reduced}
            onClick={() => setPaused((p) => !p)}
          >
            <Icon name={paused ? "play" : "pause"} size={18} />
          </button>
          <button
            type="button"
            className={ctrl}
            aria-label={t.hero.prev}
            data-testid="hero-prev"
            onClick={() => go(index - 1)}
          >
            <Icon name="chev-l" size={22} />
          </button>
          <div className="flex items-center gap-1.5 px-1">
            {items.map((it, i) => (
              <button
                key={it.movie.id}
                type="button"
                className="grid size-6 place-items-center"
                aria-label={t.hero.slidePosition(i + 1, n, it.movie.title)}
                aria-current={i === index ? "true" : undefined}
                data-testid="hero-dot"
                data-index={i}
                onClick={() => go(i)}
              >
                <span
                  className={`block h-1.5 rounded-full transition-all motion-reduce:transition-none ${
                    i === index ? "w-6 bg-green" : "w-1.5 bg-white/45"
                  }`}
                />
              </button>
            ))}
          </div>
          <button
            type="button"
            className={ctrl}
            aria-label={t.hero.next}
            data-testid="hero-next"
            onClick={() => go(index + 1)}
          >
            <Icon name="chev-r" size={22} />
          </button>
        </div>
      )}
    </section>
  );
}
