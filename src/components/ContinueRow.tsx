import { Link } from "@tanstack/react-router";
import { useContinueWatching, useRemoveProgress } from "../api/queries";
import type { ContinueItem } from "../api/types";
import { timeLeft } from "../lib/format";
import { formatClock } from "../lib/player";
import { Icon } from "./Icon";
import { Poster } from "./Poster";
import { RowShell } from "./MovieRow";

function ContinueCard({ item, onRemove }: { item: ContinueItem; onRemove: () => void }) {
  const { movie, progress } = item;
  const left = timeLeft(progress.positionS, progress.durationS);
  const fraction = progress.durationS > 0 ? Math.min(1, progress.positionS / progress.durationS) : 0;
  return (
    <div className="wide-item">
      <Link
        to="/play/$movieId"
        params={{ movieId: movie.id }}
        className="wide"
        aria-label={`Continuar ${movie.title}, ${left.toLowerCase()}`}
        data-card
      >
        {/* Landscape art: the background still; the poster (cropped) when YTS has none. */}
        {movie.backgroundUrl ? (
          <img src={movie.backgroundUrl} alt="" loading="lazy" decoding="async" />
        ) : (
          <Poster src={movie.coverLargeUrl ?? movie.coverUrl} title={movie.title} />
        )}
        <span className="wide-play" aria-hidden="true">
          <Icon name="play" size={22} className="ml-[3px]" />
        </span>
        <div className="absolute inset-x-3.5 bottom-4 z-[1]" aria-hidden="true">
          <h3 className="m-0 line-clamp-1 text-[17px] leading-tight font-extrabold stretch-semi">
            {movie.title}
          </h3>
          <div className="text-[12.5px] text-text-2 tnum">
            {left} · {formatClock(progress.positionS)} de {formatClock(progress.durationS)}
          </div>
        </div>
        <div className="absolute inset-x-0 bottom-0 z-[1] h-1 bg-white/18" aria-hidden="true">
          <i className="block h-full bg-green" style={{ width: `${(fraction * 100).toFixed(1)}%` }} />
        </div>
      </Link>
      <button
        type="button"
        className="wide-remove"
        aria-label={`Quitar ${movie.title} de Continuar viendo`}
        title="Quitar de Continuar viendo"
        onClick={onRemove}
      >
        <Icon name="x" size={16} />
      </button>
    </div>
  );
}

/** First home row: unfinished movies, most recent first. Hidden when empty or unavailable. */
export function ContinueRow() {
  const q = useContinueWatching();
  const remove = useRemoveProgress();
  if (!q.data || q.data.length === 0) return null;
  return (
    <RowShell title="Continuar viendo">
      {q.data.map((item) => (
        <ContinueCard key={item.movie.id} item={item} onRemove={() => remove.mutate(item.movie.id)} />
      ))}
    </RowShell>
  );
}
