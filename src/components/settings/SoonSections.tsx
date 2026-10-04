import type { Settings } from "../../api/types";
import { SetRow, SetSection, Switch } from "./controls";

// Visible so the page has its final shape; wired in later phases (docs/ROADMAP.md).

/** Ajustes › Subtítulos: phase 5. */
export function SubtitlesSection({ settings }: { settings: Settings }) {
  return (
    <SetSection
      id="s-subs"
      title="Subtítulos"
      soon
      lede="Se buscan en OpenSubtitles por el código IMDb de la película y se eligen los que mejor encajan con la versión de YTS."
    >
      <SetRow
        stack
        title="Clave de API de OpenSubtitles"
        help="Gratis en opensubtitles.com → Perfil → API consumers."
        id="subs-key-title"
      >
        <input
          className="input input-mono"
          type="password"
          aria-labelledby="subs-key-title"
          placeholder="Sin clave"
        />
      </SetRow>
      <SetRow title="Idioma preferido" id="subs-lang-title">
        <select className="select" aria-labelledby="subs-lang-title" defaultValue={settings.subtitleLang}>
          <option value="es">Español</option>
          <option value="en">English</option>
          <option value="pt">Português</option>
        </select>
      </SetRow>
      <SetRow
        title="Buscar subtítulos automáticamente"
        help="Al empezar una película se cargan en tu idioma sin pedírtelo."
      >
        <Switch label="Buscar subtítulos automáticamente" checked={settings.autoSubtitles} disabled />
      </SetRow>
    </SetSection>
  );
}

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
