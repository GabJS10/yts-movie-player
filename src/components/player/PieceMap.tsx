import type { PieceMapWindow } from "../../api/types";
import { parsePieceMap, windowCaption } from "../../lib/player";

/**
 * 200 cells over a window ahead of the read position (~330 KB each): ready pieces fill
 * sequentially ahead of the playhead. The "arriving" style stays for pieceMap "3", reserved by the backend.
 */
export function PieceMap({ map, window }: { map: string | null; window: PieceMapWindow | null }) {
  const cells = parsePieceMap(map);
  const caption = windowCaption(window);
  return (
    <div>
      <div className="pieces mb-2.5" aria-hidden="true" data-testid="piece-map">
        {cells.map((c, i) => (
          <i key={i} className={c === "missing" ? undefined : c} />
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-x-[18px] gap-y-1.5 text-xs text-muted">
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block h-[13px] w-[9px] rounded-[1px] bg-green" />
          Piezas listas
        </span>
        <span className="inline-flex items-center gap-1.5">
          <i className="inline-block h-[13px] w-[9px] rounded-[1px] shadow-[inset_0_0_0_1px_rgba(106,192,69,.7)]" />
          Prioridad (por delante de la reproducción)
        </span>
        {caption && <span className="ml-auto tnum max-[900px]:ml-0">{caption}</span>}
      </div>
    </div>
  );
}
