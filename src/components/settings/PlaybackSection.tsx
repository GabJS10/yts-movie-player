import { useState } from "react";
import { useUpdateSettings } from "../../api/queries";
import type { Quality, Settings } from "../../api/types";
import { SetRow, SetSection, Segmented, Switch } from "./controls";

const MB = 1024 * 1024;

const QUALITIES: { value: Quality; label: string }[] = [
  { value: "720p", label: "720p" },
  { value: "1080p", label: "1080p" },
  { value: "2160p", label: "4K" },
];

/** Bytes to gather before the video starts (bufferTargetBytes). */
const BUFFER_OPTIONS_MB = [4, 8, 16, 32] as const;

/** Ajustes › Reproducción: default version, external player and the start buffer. */
export function PlaybackSection({ settings }: { settings: Settings }) {
  const update = useUpdateSettings();
  // Edited locally; saved on blur or Enter. Re-seeded when the stored value changes elsewhere.
  const [player, setPlayer] = useState(settings.externalPlayer);
  const [seen, setSeen] = useState(settings.externalPlayer);
  if (seen !== settings.externalPlayer) {
    setSeen(settings.externalPlayer);
    setPlayer(settings.externalPlayer);
  }
  const playerError = player.trim() ? null : "Escribe el comando o la ruta del reproductor.";

  const commitPlayer = () => {
    const value = player.trim();
    if (!value) return;
    if (value !== settings.externalPlayer) update.mutate({ externalPlayer: value });
    else setPlayer(value);
  };

  const bufferMb = Math.round(settings.bufferTargetBytes / MB);

  return (
    <SetSection id="s-play" title="Reproducción">
      <SetRow title="Calidad preferida" help="Se elige por defecto en la ficha si existe.">
        <Segmented
          label="Calidad preferida"
          value={settings.preferredQuality}
          options={QUALITIES}
          onChange={(preferredQuality) => update.mutate({ preferredQuality })}
        />
      </SetRow>
      <SetRow
        title="Preferir x264"
        help="Las versiones x265 (HEVC, típicas en 4K) no suelen reproducirse en el reproductor integrado."
      >
        <Switch
          label="Preferir x264"
          checked={settings.preferX264}
          onChange={(preferX264) => update.mutate({ preferX264 })}
        />
      </SetRow>
      <SetRow
        title="Reproductor externo"
        help="Para «Abrir en VLC» cuando el reproductor integrado no puede con un archivo."
        id="external-player-title"
      >
        <div className="grid justify-items-end gap-1">
          <input
            className="input input-mono w-[220px]"
            aria-labelledby="external-player-title"
            aria-invalid={playerError ? true : undefined}
            value={player}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => setPlayer(e.target.value)}
            onBlur={commitPlayer}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitPlayer();
              if (e.key === "Escape") setPlayer(settings.externalPlayer);
            }}
          />
          {playerError && <span className="text-[12.5px] text-danger">{playerError}</span>}
        </div>
      </SetRow>
      <SetRow
        title="Búfer antes de empezar"
        help="Cuánto se descarga antes de dar al play. Más búfer tarda más en arrancar, pero se corta menos con pocos seeds."
      >
        <Segmented
          label="Búfer antes de empezar"
          value={BUFFER_OPTIONS_MB.find((mb) => mb === bufferMb) ?? null}
          options={BUFFER_OPTIONS_MB.map((mb) => ({ value: mb, label: `${mb} MB` }))}
          onChange={(mb) => update.mutate({ bufferTargetBytes: mb * MB })}
        />
      </SetRow>
    </SetSection>
  );
}
