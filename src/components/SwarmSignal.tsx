import { formatCount } from "../lib/format";
import { LOW_SEEDS, signalLevel } from "../lib/versions";

type Props = { seeds: number; peers?: number; withText?: boolean };

/** Five-bar swarm health. Bars are always green; state is form: solid held, hollow missing, struck dead. */
export function SwarmSignal({ seeds, peers, withText = true }: Props) {
  const level = signalLevel(seeds);
  const label = level === 0 ? "Sin seeds" : `${formatCount(seeds)} seeds`;
  const title = `Salud del enjambre: ${label}${peers === undefined ? "" : `, ${formatCount(peers)} peers`}`;
  return (
    <span className={`signal ${level === 0 ? "signal-dead" : ""}`} title={title}>
      <span className="signal-bars" aria-hidden="true">
        {[1, 2, 3, 4, 5].map((i) => (
          <i key={i} className={i <= level ? "on" : undefined} />
        ))}
      </span>
      {withText ? (
        <span>
          <b>{label}</b>
          {level > 0 && seeds < LOW_SEEDS ? " · pocos seeds" : ""}
        </span>
      ) : (
        <span className="sr-only">{label}</span>
      )}
    </span>
  );
}
