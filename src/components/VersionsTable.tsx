import { useRef, type KeyboardEvent } from "react";
import type { Torrent } from "../api/types";
import { formatBytes, formatCount } from "../lib/format";
import { SOURCE_LABEL, torrentNotes } from "../lib/versions";
import { Icon } from "./Icon";
import { SwarmSignal } from "./SwarmSignal";

type Props = {
  torrents: Torrent[];
  selected: string | null;
  onSelect: (infohash: string) => void;
  labelledBy: string;
};

/** Ruled timetable of versions, one radio per row (↑/↓ to move). */
export function VersionsTable({ torrents, selected, onSelect, labelledBy }: Props) {
  const rows = useRef<(HTMLTableRowElement | null)[]>([]);
  const current = Math.max(
    0,
    torrents.findIndex((t) => t.infohash === selected),
  );

  const onKeyDown = (e: KeyboardEvent<HTMLTableRowElement>, i: number) => {
    const step: Record<string, number> = {
      ArrowDown: 1,
      ArrowRight: 1,
      ArrowUp: -1,
      ArrowLeft: -1,
      " ": 0,
      Enter: 0,
    };
    const delta = step[e.key];
    if (delta === undefined) return;
    const next = (i + delta + torrents.length) % torrents.length;
    e.preventDefault();
    const t = torrents[next];
    if (!t) return;
    onSelect(t.infohash);
    rows.current[next]?.focus();
  };

  return (
    <div className="overflow-x-auto border-t border-line [contain:inline-size]">
      <table className="tt" role="radiogroup" aria-labelledby={labelledBy}>
        <thead>
          <tr>
            <th className="w-11">
              <span className="sr-only">Elegida</span>
            </th>
            <th>Calidad</th>
            <th>Fuente</th>
            <th>Códec</th>
            <th>Audio</th>
            <th className="r">Tamaño</th>
            <th className="r">Seeds</th>
            <th className="r">Peers</th>
            <th>Enjambre</th>
            <th>
              <span className="sr-only">Avisos</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {torrents.map((t, i) => {
            const checked = t.infohash === selected;
            const note = torrentNotes(t)[0];
            return (
              <tr
                key={t.infohash}
                ref={(el) => {
                  rows.current[i] = el;
                }}
                role="radio"
                aria-checked={checked}
                data-testid="version-row"
                data-infohash={t.infohash}
                aria-label={`${t.quality} ${SOURCE_LABEL[t.source]} ${t.videoCodec}, ${formatBytes(t.sizeBytes)}, ${formatCount(t.seeds)} seeds`}
                tabIndex={i === current ? 0 : -1}
                onClick={() => onSelect(t.infohash)}
                onKeyDown={(e) => onKeyDown(e, i)}
              >
                <td>
                  <span className="radio" />
                </td>
                <td>
                  <span className="text-base font-[850] text-text">{t.quality}</span>
                </td>
                <td>{SOURCE_LABEL[t.source]}</td>
                <td>
                  {t.videoCodec}
                  {t.bitDepth === 10 ? " · 10 bit" : ""}
                </td>
                <td>{t.audioChannels ?? "—"}</td>
                <td className="r">{formatBytes(t.sizeBytes)}</td>
                <td className="r">{formatCount(t.seeds)}</td>
                <td className="r">{formatCount(t.peers)}</td>
                <td>
                  <SwarmSignal seeds={t.seeds} peers={t.peers} withText={false} />
                </td>
                <td>
                  {note && (
                    <span className="inline-flex items-center gap-1.5 text-[12.5px] text-warn">
                      <Icon name="alert" size={15} />
                      {note.text}
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
