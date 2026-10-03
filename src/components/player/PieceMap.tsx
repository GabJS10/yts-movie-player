import { parsePieceMap } from "../../lib/player";

/** 200-cell sample of the torrent: ready pieces fill sequentially ahead of the playhead. */
export function PieceMap({ map }: { map: string | null }) {
  const cells = parsePieceMap(map);
  return (
    <div>
      <div className="pieces mb-2.5" aria-hidden="true" data-testid="piece-map">
        {cells.map((c, i) => (
          <i key={i} className={c === "missing" ? undefined : c} />
        ))}
      </div>
      <div className="flex flex-wrap gap-x-[18px] gap-y-1.5 text-xs text-muted">
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block h-[13px] w-[9px] rounded-[1px] bg-green" />
          Piezas listas
        </span>
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block h-[13px] w-[9px] rounded-[1px] shadow-[inset_0_0_0_1px_rgba(106,192,69,.7)]" />
          Prioridad (por delante de la reproducción)
        </span>
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block h-[13px] w-[9px] rounded-[1px] bg-[#4d4d4d]" />
          Llegando de otros peers
        </span>
      </div>
    </div>
  );
}
