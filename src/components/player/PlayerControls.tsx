import { useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import type { Torrent, TorrentStats } from "../../api/types";
import { formatSpeed } from "../../lib/format";
import { formatClock, type ScrubLayers } from "../../lib/player";
import { Icon } from "../Icon";
import { ReleaseTag } from "../ReleaseTag";
import { SwarmSignal } from "../SwarmSignal";

type Props = {
  title: string;
  torrent: Torrent;
  stats: TorrentStats | null;
  playing: boolean;
  currentTime: number;
  duration: number;
  layers: ScrubLayers;
  volume: number;
  muted: boolean;
  fullscreen: boolean;
  onToggle: () => void;
  onSeek: (seconds: number) => void;
  onSkip: (delta: number) => void;
  onVolume: (v: number) => void;
  onMute: () => void;
  onFullscreen: () => void;
  onBack: () => void;
  /** Subtitle menu (cc button + popover), before fullscreen. */
  subtitles?: ReactNode;
  /** "Abrir en VLC" from the same local stream. */
  onOpenExternal: () => void;
  /** VLC is being launched: the button waits. */
  externalOpening?: boolean;
};

const pct = (n: number) => `${(n * 100).toFixed(3)}%`;

export function ScrubBar({
  layers,
  currentTime,
  duration,
  onSeek,
}: Pick<Props, "layers" | "currentTime" | "duration" | "onSeek">) {
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<number | null>(null);
  const at = (e: MouseEvent) => {
    const r = ref.current?.getBoundingClientRect();
    return r && r.width ? Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) : 0;
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    e.stopPropagation();
    onSeek(Math.min(duration, Math.max(0, currentTime + (e.key === "ArrowLeft" ? -10 : 10))));
  };
  return (
    <div
      ref={ref}
      className="scrub"
      role="slider"
      tabIndex={0}
      aria-label="Posición"
      data-testid="player-scrub"
      aria-valuemin={0}
      aria-valuemax={Math.round(duration) || 0}
      aria-valuenow={Math.round(currentTime)}
      aria-valuetext={`${formatClock(currentTime)} de ${formatClock(duration)}`}
      onMouseMove={(e) => setHover(at(e))}
      onMouseLeave={() => setHover(null)}
      onClick={(e) => duration && onSeek(at(e) * duration)}
      onKeyDown={onKey}
    >
      <div className="scrub-rail" data-testid="scrub-rail">
        {layers.available.map((s, i) => (
          <i
            key={`a${i}`}
            className="available"
            style={{ left: pct(s.start), width: pct(s.end - s.start) }}
          />
        ))}
        {layers.buffered.map((s, i) => (
          <i key={`b${i}`} className="buffered" style={{ left: pct(s.start), width: pct(s.end - s.start) }} />
        ))}
        <i className="played" style={{ width: pct(layers.played) }} />
        <i className="scrub-knob" style={{ left: pct(layers.played) }} />
      </div>
      {hover !== null && duration > 0 && (
        <span className="scrub-tip" style={{ left: pct(hover) }}>
          {formatClock(hover * duration)}
        </span>
      )}
    </div>
  );
}

const Legend = () => (
  <div className="mt-0.5 mb-1.5 flex gap-4 text-xs text-muted opacity-0 transition-opacity group-has-[.scrub:hover]/bottom:opacity-100 group-has-[.scrub:focus-visible]/bottom:opacity-100">
    <span className="inline-flex items-center gap-1.5">
      <i className="inline-block h-1 w-3.5 rounded-[1px] bg-green" />
      Visto
    </span>
    <span className="inline-flex items-center gap-1.5">
      <i className="inline-block h-1 w-3.5 rounded-[1px] bg-white/55" />
      En búfer
    </span>
    <span className="inline-flex items-center gap-1.5">
      <i className="inline-block h-2 w-3.5 rounded-[1px] shadow-[inset_0_0_0_1px_rgba(255,255,255,.7)]" />
      Ya descargado: saltar ahí es inmediato
    </span>
  </div>
);

export function PlayerControls(p: Props) {
  return (
    <div className="pointer-events-none absolute inset-0 flex flex-col justify-between [&>*]:pointer-events-auto">
      <div className="flex items-center gap-[18px] bg-linear-to-b from-black/75 to-transparent px-gutter pt-[22px] pb-[60px]">
        <button
          type="button"
          className="ctrl-btn"
          aria-label="Salir del reproductor"
          data-testid="player-back"
          onClick={p.onBack}
        >
          <Icon name="back" size={22} />
        </button>
        <h1 className="m-0 text-xl font-[750]">{p.title}</h1>
        <ReleaseTag torrent={p.torrent} />
        {p.stats && (
          <div className="ml-auto flex items-center gap-4 text-[13px] text-text-2 tnum max-[900px]:hidden">
            <span className="inline-flex items-center gap-0.5">
              <Icon name="down" size={14} />
              {formatSpeed(p.stats.downSpeedBps)}
            </span>
            <span>{p.stats.peers} peers</span>
            <SwarmSignal seeds={p.stats.seeds} peers={p.stats.peers} withText={false} />
          </div>
        )}
      </div>

      <div className="group/bottom bg-linear-to-t from-black/85 to-transparent px-gutter pt-[70px] pb-[22px]">
        <ScrubBar layers={p.layers} currentTime={p.currentTime} duration={p.duration} onSeek={p.onSeek} />
        <Legend />
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            className="ctrl-btn"
            aria-label={p.playing ? "Pausar (Espacio)" : "Reproducir (Espacio)"}
            data-testid="player-play-toggle"
            onClick={p.onToggle}
          >
            <Icon name={p.playing ? "pause" : "play"} size={26} />
          </button>
          <button
            type="button"
            className="ctrl-btn"
            aria-label="Retroceder 10 segundos (←)"
            data-testid="player-back-10"
            onClick={() => p.onSkip(-10)}
          >
            <Icon name="rew10" size={26} />
          </button>
          <button
            type="button"
            className="ctrl-btn"
            aria-label="Adelantar 10 segundos (→)"
            data-testid="player-forward-10"
            onClick={() => p.onSkip(10)}
          >
            <Icon name="fwd10" size={26} />
          </button>
          <div className="group/vol flex items-center">
            <button
              type="button"
              className="ctrl-btn"
              aria-label={p.muted ? "Activar sonido (M)" : "Silenciar (M)"}
              onClick={p.onMute}
            >
              <Icon name={p.muted || p.volume === 0 ? "mute" : "volume"} size={26} />
            </button>
            <input
              type="range"
              min={0}
              max={100}
              value={Math.round((p.muted ? 0 : p.volume) * 100)}
              onChange={(e) => p.onVolume(Number(e.target.value) / 100)}
              aria-label="Volumen"
              className="w-0 accent-green opacity-0 transition-all group-focus-within/vol:w-[90px] group-focus-within/vol:opacity-100 group-hover/vol:w-[90px] group-hover/vol:opacity-100"
            />
          </div>
          <span className="ml-2.5 text-sm whitespace-nowrap text-text-2 tnum" data-testid="clock">
            {formatClock(p.currentTime)} / {formatClock(p.duration)}
          </span>
          <span className="mx-auto truncate px-4 text-[15px] font-semibold text-text-2 max-[900px]:hidden">
            {p.title}
          </span>
          <span className="ml-auto" />
          {p.subtitles}
          <button
            type="button"
            className="ctrl-btn"
            aria-label="Abrir en VLC (V)"
            data-testid="player-open-external"
            onClick={p.onOpenExternal}
            disabled={p.externalOpening}
          >
            <Icon name="external" size={26} />
          </button>
          <button
            type="button"
            className="ctrl-btn"
            aria-label={p.fullscreen ? "Salir de pantalla completa (F)" : "Pantalla completa (F)"}
            data-testid="player-fullscreen"
            onClick={p.onFullscreen}
          >
            <Icon name={p.fullscreen ? "fullscreen-exit" : "fullscreen"} size={26} />
          </button>
        </div>
      </div>
    </div>
  );
}
