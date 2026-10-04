import type { Settings } from "../../api/types";
import { SetRow, SetSection, Switch } from "./controls";

// Visible so the page has its final shape; wired in later phases (docs/ROADMAP.md).

/** Ajustes › Torrent: phase 6. */
export function TorrentSection({ settings }: { settings: Settings }) {
  const unit = "inline-flex items-center gap-2 text-[13px] text-muted";
  const num = "input w-24 text-right tnum";
  return (
    <SetSection id="s-torrent" title="Torrent" soon>
      <SetRow title="Límite de descarga" help="Vacío = sin límite.">
        <label className={unit}>
          <input
            className={num}
            type="number"
            aria-label="Límite de descarga"
            defaultValue={settings.downLimitKbps ?? ""}
          />
          KB/s
        </label>
      </SetRow>
      <SetRow title="Límite de subida">
        <label className={unit}>
          <input
            className={num}
            type="number"
            aria-label="Límite de subida"
            defaultValue={settings.upLimitKbps ?? ""}
          />
          KB/s
        </label>
      </SetRow>
      <SetRow
        title="Seguir compartiendo al terminar"
        help="Ayuda a que otros puedan ver la película. Usa subida mientras la app está abierta."
      >
        <Switch label="Seguir compartiendo al terminar" checked={settings.seedAfterDownload} disabled />
      </SetRow>
      <SetRow title="Puerto de escucha" help="Vacío = automático.">
        <input
          className="input w-[110px] text-right tnum"
          type="number"
          aria-label="Puerto de escucha"
          defaultValue={settings.listenPort ?? ""}
        />
      </SetRow>
    </SetSection>
  );
}
