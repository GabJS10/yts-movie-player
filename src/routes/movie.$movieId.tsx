import { createFileRoute, notFound } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useMovie, useSuggestions } from "../api/queries";
import type { MovieDetail } from "../api/types";
import { ErrorState } from "../components/ErrorState";
import { Icon } from "../components/Icon";
import { MovieMeta } from "../components/MovieMeta";
import { Poster } from "../components/Poster";
import { StaticMovieRow } from "../components/MovieRow";
import { VersionsTable } from "../components/VersionsTable";
import { genreLabel } from "../lib/genres";
import { pickDefaultTorrent, sortForDisplay } from "../lib/versions";

export const Route = createFileRoute("/movie/$movieId")({
  params: {
    parse: ({ movieId }) => {
      const id = Number(movieId);
      if (!Number.isInteger(id) || id <= 0) throw notFound();
      return { movieId: id };
    },
    stringify: ({ movieId }) => ({ movieId: String(movieId) }),
  },
  component: MoviePage,
});

function MoviePage() {
  const { movieId } = Route.useParams();
  const movie = useMovie(movieId);
  const suggestions = useSuggestions(movieId);

  if (movie.isError) {
    return (
      <div className="px-gutter pt-[calc(var(--spacing-nav)+36px)]">
        <ErrorState error={movie.error} onRetry={() => void movie.refetch()} />
      </div>
    );
  }
  return (
    <>
      {movie.data ? <MovieBody key={movie.data.id} movie={movie.data} /> : <MovieSkeleton />}
      <div className="mt-12 pb-20">
        <StaticMovieRow title="Similares" movies={suggestions.data} loading={suggestions.isPending} />
      </div>
    </>
  );
}

const soon = "Disponible en una próxima versión";

function MovieBody({ movie }: { movie: MovieDetail }) {
  const torrents = useMemo(() => sortForDisplay(movie.torrents), [movie.torrents]);
  const [selected, setSelected] = useState(() => pickDefaultTorrent(movie.torrents)?.infohash ?? null);
  const chosen = torrents.find((t) => t.infohash === selected);
  // Sharp still first; the blurred background next; the poster (blurred further) as a last resort.
  const still = movie.screenshotUrls[0] ?? movie.backgroundUrl;
  const art = still ?? movie.coverLargeUrl ?? movie.coverUrl;

  return (
    <article>
      <div className="relative h-[54vh] min-h-[380px] overflow-hidden">
        {art && (
          <img
            src={art}
            alt=""
            decoding="async"
            className={`size-full object-cover object-[center_35%] ${still ? "" : "scale-125 opacity-70 blur-[28px]"}`}
          />
        )}
        <div className="absolute inset-0 bg-[linear-gradient(0deg,var(--color-ground)_0%,rgba(23,23,23,.6)_45%,rgba(23,23,23,.15)_100%),linear-gradient(90deg,rgba(23,23,23,.75),rgba(23,23,23,0)_60%)]" />
      </div>

      <div className="relative z-[2] -mt-[38vh] grid grid-cols-[clamp(180px,18vw,260px)_minmax(0,1fr)] gap-x-[clamp(24px,3.2vw,48px)] overflow-x-clip px-gutter max-[900px]:-mt-[22vh] max-[900px]:grid-cols-[minmax(0,1fr)]">
        <div className="self-start overflow-hidden rounded-lg shadow-[0_24px_48px_rgba(0,0,0,.6)] max-[900px]:mb-5 max-[900px]:w-[140px]">
          <div className="aspect-[2/3]" role="img" aria-label={`Póster de ${movie.title}`}>
            <Poster src={movie.coverLargeUrl ?? movie.coverUrl} title={movie.title} eager />
          </div>
        </div>

        <div>
          <h1 className="m-0 mb-3.5 text-display-detail font-[850] text-balance uppercase stretch-condensed">
            {movie.title}
          </h1>
          <MovieMeta movie={movie} mpaRating={movie.mpaRating} />
          <ul className="m-0 mt-3 flex list-none flex-wrap gap-x-4 gap-y-1.5 p-0 text-sm text-muted [&>li+li]:before:mr-4 [&>li+li]:before:text-faint [&>li+li]:before:content-['·']">
            {movie.genres.map((g) => (
              <li key={g}>{genreLabel(g)}</li>
            ))}
          </ul>

          <div className="my-7 flex flex-wrap gap-3">
            <button type="button" className="btn btn-play" disabled title={soon}>
              <Icon name="play" size={22} />
              {chosen ? `Reproducir ${chosen.quality}` : "Reproducir"}
            </button>
            <button type="button" className="btn btn-line" disabled title={soon}>
              <Icon name={movie.isFavorite ? "heart" : "plus"} size={22} />
              {movie.isFavorite ? "En Mi lista" : "Mi lista"}
            </button>
            <button type="button" className="btn btn-line" disabled title={soon}>
              <Icon name="download" size={22} />
              Descargar
            </button>
          </div>

          <p className="m-0 mb-5 max-w-[68ch] text-lead text-text-2">
            {movie.summary || "Sin sinopsis disponible."}
          </p>
          {movie.cast.length > 0 && (
            <p className="m-0 flex flex-wrap gap-x-2.5 gap-y-1 text-sm text-muted">
              <span>Reparto:</span>
              {movie.cast.map((c, i) => (
                <span key={`${c.name}-${i}`}>
                  <b className="font-semibold text-text-2">{c.name}</b>
                  {c.character ? ` (${c.character})` : ""}
                </span>
              ))}
            </p>
          )}
        </div>

        <section className="col-span-full mt-12 min-w-0" aria-labelledby="versions-title">
          <h2 id="versions-title" className="m-0 mb-3.5 text-title font-[750]">
            Elige versión
            <small className="ml-3 text-[13px] font-medium text-muted max-[900px]:mt-1 max-[900px]:ml-0 max-[900px]:block">
              Los seeds son quienes tienen el archivo completo; más seeds, arranque más rápido.
            </small>
          </h2>
          {torrents.length > 0 ? (
            <VersionsTable
              torrents={torrents}
              selected={selected}
              onSelect={setSelected}
              labelledBy="versions-title"
            />
          ) : (
            <p className="text-muted">YTS no tiene versiones disponibles para esta película.</p>
          )}
        </section>
      </div>
    </article>
  );
}

function MovieSkeleton() {
  return (
    <div aria-busy="true" aria-label="Cargando película">
      <div className="skeleton h-[54vh] min-h-[380px] rounded-none" />
      <div className="relative -mt-[38vh] grid grid-cols-[clamp(180px,18vw,260px)_minmax(0,1fr)] gap-x-12 px-gutter max-[900px]:-mt-[22vh] max-[900px]:grid-cols-1">
        <div className="skeleton aspect-[2/3] max-[900px]:w-[140px]" />
        <div className="pt-6">
          <div className="skeleton mb-4 h-16 w-[60%]" />
          <div className="skeleton mb-8 h-5 w-[40%]" />
          <div className="skeleton mb-6 h-12 w-[70%]" />
          <div className="skeleton h-20 w-[85%]" />
        </div>
      </div>
    </div>
  );
}
