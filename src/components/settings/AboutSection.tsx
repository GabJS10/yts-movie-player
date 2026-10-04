import { useAppInfo, useUpdateCheck } from "../../api/queries";
import { openExternalUrl, openLogsFolder } from "../../api/tauri";
import { showToast } from "../../store/toast";
import { Icon } from "../Icon";
import { SetRow, SetSection } from "./controls";

/** Who makes what the app shows and plays possible. */
const CREDITS: { name: string; role: string; url: string }[] = [
  { name: "YTS", role: "catálogo de películas y torrents", url: "https://yts.mx" },
  { name: "OpenSubtitles", role: "subtítulos", url: "https://www.opensubtitles.com" },
  { name: "librqbit", role: "motor BitTorrent", url: "https://github.com/ikatson/rqbit" },
  { name: "Tauri", role: "aplicación de escritorio", url: "https://tauri.app" },
];

const open = (url: string) =>
  void openExternalUrl(url).catch(() => showToast("No se pudo abrir el navegador", "error"));

const linkBtn = "font-semibold text-green hover:underline";

/** Ajustes › Acerca de: version (and a newer one, if any), repository, logs, legal notice and credits. */
export function AboutSection() {
  const info = useAppInfo().data;
  const update = useUpdateCheck().data;

  const openLogs = () =>
    void openLogsFolder().catch(() => showToast("No se pudo abrir la carpeta de registros", "error"));

  return (
    <SetSection id="s-about" title="Acerca de">
      <SetRow
        title="Versión"
        help={
          update ? (
            <>
              Hay una versión nueva: <b className="text-text-2">{update.version}</b>.{" "}
              <button type="button" className={linkBtn} onClick={() => open(update.url)}>
                Ver la versión {update.version}
              </button>
            </>
          ) : (
            "Tienes la última versión."
          )
        }
      >
        <span className="text-[15px] font-bold tnum" data-testid="app-version">
          {info ? info.version : "—"}
        </span>
      </SetRow>
      <SetRow title="Código fuente" help="El proyecto en GitHub: novedades, versiones y avisos de errores.">
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={!info}
          onClick={() => info && open(info.repoUrl)}
        >
          <Icon name="external" size={18} />
          Abrir en GitHub
        </button>
      </SetRow>
      <SetRow
        title="Registros"
        help={
          <>
            Para adjuntarlos si avisas de un problema. No incluyen tu clave ni tu cuenta de OpenSubtitles.
            {info && (
              <code className="mt-1 block font-mono text-[12.5px] break-all text-muted">{info.logsDir}</code>
            )}
          </>
        }
      >
        <button type="button" className="btn btn-line btn-sm" data-testid="open-logs" onClick={openLogs}>
          <Icon name="folder" size={18} />
          Abrir carpeta de registros
        </button>
      </SetRow>
      <SetRow
        stack
        title="Aviso legal"
        help="YTS Player no aloja ni distribuye películas: muestra el catálogo público de YTS y descarga los archivos por BitTorrent desde otras personas que los comparten. Tú eres responsable de usarlo de acuerdo con las leyes de propiedad intelectual de tu país."
      />
      <SetRow stack title="Créditos">
        <ul className="m-0 grid list-none gap-1.5 p-0 text-[14px] text-muted">
          {CREDITS.map((c) => (
            <li key={c.name}>
              <button type="button" className={linkBtn} onClick={() => open(c.url)}>
                {c.name}
              </button>
              : {c.role}
            </li>
          ))}
        </ul>
      </SetRow>
    </SetSection>
  );
}
