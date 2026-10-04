import type {
  ExternalSubtitleArgs,
  MovieDetail,
  StreamSession,
  Torrent,
  TorrentStats,
} from "../../api/types";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { formatCount, formatSpeed } from "../../lib/format";
import { PHASE_TEXT } from "../../lib/player";
import { betterSwarm, SOURCE_LABEL } from "../../lib/versions";
import { Icon } from "../Icon";
import { ReleaseTag } from "../ReleaseTag";
import { SwarmSignal } from "../SwarmSignal";
import { ExternalPlayerActions } from "./ExternalPlayerActions";
import { PieceMap } from "./PieceMap";

type Props = {
  movie: MovieDetail;
  torrent: Torrent;
  session: StreamSession | null;
  stats: TorrentStats | null;
  alternative: Torrent | null;
  /** Active subtitle for "Abrir en VLC". */
  subtitles?: ExternalSubtitleArgs;
  resumeAtS: number | null;
  onBack: () => void;
};

const nf1 = new Intl.NumberFormat("es-ES", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
const MB = 1024 * 1024;

/** Pre-roll: the swarm fills the buffer ahead of the playhead before the first frame. */
export function BufferScreen({
  movie,
  torrent,
  session,
  stats,
  alternative,
  subtitles,
  resumeAtS,
  onBack,
}: Props) {
  const target = session?.bufferTargetBytes ?? 8 * MB;
  const buffered = Math.min(stats?.bufferedAheadBytes ?? 0, target);
  const phase = stats ? PHASE_TEXT[stats.phase] : "Conectando al enjambre…";
  const noPeers = stats?.phase === "no_peers";
  const stalled = stats?.phase === "stalled" || noPeers;
  // no_peers is a decision point; "Seguir esperando" goes back to the regular screen (the torrent keeps trying).
  const [keepWaiting, setKeepWaiting] = useState(false);
  if (noPeers && !keepWaiting) {
    return <NoPeers movie={movie} torrent={torrent} onWait={() => setKeepWaiting(true)} onBack={onBack} />;
  }
  return (
    <div className="absolute inset-0 grid place-items-center overflow-y-auto p-6">
      <div className="w-full max-w-[640px]" aria-live="polite">
        <h1 className="m-0 mb-1 text-[clamp(2rem,4vw,3.25rem)] leading-[.95] font-[850] uppercase stretch-condensed">
          {movie.title}
        </h1>
        <div className="mt-3.5 mb-8 flex flex-wrap items-center gap-2.5">
          <ReleaseTag torrent={torrent} on />
          {/* Seeds are YTS's static count (IPC v0.5); only peers are live. */}
          <SwarmSignal seeds={torrent.seeds} peers={torrent.peers} />
          {resumeAtS ? <span className="text-[13px] text-muted">Retomando donde lo dejaste</span> : null}
        </div>

        {session && !session.likelyPlayable && (
          <div className="mb-7 border-y border-line py-4">
            <p className="m-0 mb-3 text-sm text-text-2">
              Esta versión es {torrent.videoCodec} (HEVC): el reproductor integrado probablemente no pueda
              mostrarla. Puedes abrirla en VLC desde ya, mientras se descarga.
            </p>
            <ExternalPlayerActions
              movieId={movie.id}
              infohash={torrent.infohash}
              alternative={alternative}
              subtitles={subtitles}
              primary={false}
            />
          </div>
        )}

        <p className="m-0 mb-3 flex items-center gap-2.5 text-[15px] font-semibold">
          {stalled ? (
            <Icon name="alert" size={16} className="flex-none text-warn" />
          ) : (
            <span className="pulse-dot" aria-hidden="true" />
          )}
          <span data-testid="phase">{phase}</span>
        </p>
        <PieceMap map={stats?.pieceMap ?? null} window={stats?.pieceMapWindow ?? null} />

        <dl className="mt-7 grid grid-cols-3 border-t border-line">
          <div className="pt-3.5">
            <dt className="field-label">Peers</dt>
            <dd className="m-0 mt-0.5 text-[28px] font-extrabold stretch-semi tnum max-[900px]:text-xl">
              {stats?.peers ?? 0}
            </dd>
          </div>
          <div className="border-l border-line pt-3.5 pl-5">
            <dt className="field-label">Velocidad</dt>
            <dd className="m-0 mt-0.5 text-[28px] font-extrabold stretch-semi tnum max-[900px]:text-xl">
              {formatSpeed(stats?.downSpeedBps ?? 0).replace(" MB/s", "")}
              <small className="ml-1 text-sm font-semibold text-muted">MB/s</small>
            </dd>
          </div>
          <div className="border-l border-line pt-3.5 pl-5">
            <dt className="field-label">Búfer</dt>
            <dd className="m-0 mt-0.5 text-[28px] font-extrabold stretch-semi tnum max-[900px]:text-xl">
              {nf1.format(buffered / MB)}
              <small className="ml-1 text-sm font-semibold text-muted">/ {Math.round(target / MB)} MB</small>
            </dd>
          </div>
        </dl>

        <div className="mt-8 flex flex-wrap items-center justify-between gap-3 text-[13px] text-muted">
          <button type="button" className="btn btn-line btn-sm" onClick={onBack}>
            <Icon name="back" size={18} />
            Volver
          </button>
          <span>
            {noPeers
              ? "Seguimos buscando a alguien que la comparta."
              : stalled
                ? "Si sigue sin datos, prueba otra versión con más seeds."
                : `Empieza solo al tener ${Math.round(target / MB)} MB · `}
            {!stalled && (
              <>
                <kbd className="rounded-sm border border-line-hi px-1.5 text-[11px] font-bold text-text-2">
                  Enter
                </kbd>{" "}
                para empezar ya
              </>
            )}
          </span>
        </div>
      </div>
    </div>
  );
}

/** 60 s without ever reaching a peer: another version with more seeds, keep waiting, or go back. */
function NoPeers({
  movie,
  torrent,
  onWait,
  onBack,
}: {
  movie: MovieDetail;
  torrent: Torrent;
  onWait: () => void;
  onBack: () => void;
}) {
  const other = betterSwarm(movie.torrents, torrent);
  return (
    <div className="absolute inset-0 grid place-items-center overflow-y-auto p-6" data-testid="no-peers">
      <div className="w-full max-w-[600px]" role="alert">
        <p className="m-0 mb-2 text-[13px] font-semibold tracking-[0.08em] text-muted uppercase">
          {movie.title} · {torrent.quality} {torrent.videoCodec}
        </p>
        <h1 className="m-0 mb-3 flex items-center gap-3 text-[clamp(1.6rem,3vw,2.2rem)] leading-tight font-[850]">
          <Icon name="alert" size={28} className="flex-none text-warn" />
          Nadie está compartiendo esta versión
        </h1>
        <p className="m-0 mb-7 text-[15px] text-text-2">
          Llevamos un minuto sin encontrar a nadie que la tenga. El torrent sigue intentándolo por si aparece
          alguien, pero lo más rápido es probar otra versión.
        </p>
        <div className="flex flex-wrap gap-3">
          {other && (
            <Link
              to="/play/$movieId"
              params={{ movieId: movie.id }}
              search={{ infohash: other.infohash }}
              replace
              className="btn btn-play"
              data-testid="no-peers-alternative"
            >
              <Icon name="play" size={22} />
              Probar {other.quality} {SOURCE_LABEL[other.source]} {other.videoCodec} ·{" "}
              {formatCount(other.seeds)} seeds
            </Link>
          )}
          <button type="button" className="btn btn-line" data-testid="no-peers-wait" onClick={onWait}>
            <Icon name="refresh" size={22} />
            Seguir esperando
          </button>
          <button type="button" className="btn btn-line" data-testid="no-peers-back" onClick={onBack}>
            <Icon name="back" size={22} />
            Volver
          </button>
        </div>
        {!other && (
          <p className="m-0 mt-5 text-sm text-muted">Ninguna otra versión tiene más seeds ahora mismo.</p>
        )}
      </div>
    </div>
  );
}
