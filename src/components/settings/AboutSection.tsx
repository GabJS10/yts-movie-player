import { useAppInfo, useUpdateCheck } from "../../api/queries";
import { openExternalUrl, openLogsFolder } from "../../api/tauri";
import { getT, useT, type Messages } from "../../i18n";
import { showToast } from "../../store/toast";
import { Icon } from "../Icon";
import { SetRow, SetSection } from "./controls";

/** Who makes what the app shows and plays possible. */
const CREDITS: { name: string; role: keyof Messages["about"]["credits"]; url: string }[] = [
  { name: "YTS", role: "yts", url: "https://yts.mx" },
  { name: "OpenSubtitles", role: "subtitles", url: "https://www.opensubtitles.com" },
  { name: "librqbit", role: "torrent", url: "https://github.com/ikatson/rqbit" },
  { name: "Tauri", role: "desktop", url: "https://tauri.app" },
];

const open = (url: string) =>
  void openExternalUrl(url).catch(() => showToast(getT().about.browserFailed, "error"));

const linkBtn = "font-semibold text-green hover:underline";

/** Ajustes › Acerca de: version (and a newer one, if any), repository, logs, legal notice and credits. */
export function AboutSection() {
  const info = useAppInfo().data;
  const update = useUpdateCheck().data;
  const t = useT().about;

  const openLogs = () => void openLogsFolder().catch(() => showToast(t.logsFailed, "error"));

  return (
    <SetSection id="s-about" title={t.title}>
      <SetRow
        title={t.version}
        help={
          update ? (
            <>
              {t.newVersion}
              <b className="text-text-2">{update.version}</b>.{" "}
              <button type="button" className={linkBtn} onClick={() => open(update.url)}>
                {t.seeVersion(update.version)}
              </button>
            </>
          ) : (
            t.upToDate
          )
        }
      >
        <span className="text-[15px] font-bold tnum" data-testid="app-version">
          {info ? info.version : "—"}
        </span>
      </SetRow>
      <SetRow title={t.source} help={t.sourceHelp}>
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={!info}
          onClick={() => info && open(info.repoUrl)}
        >
          <Icon name="external" size={18} />
          {t.openGithub}
        </button>
      </SetRow>
      <SetRow
        title={t.logs}
        help={
          <>
            {t.logsHelp}
            {info && (
              <code className="mt-1 block font-mono text-[12.5px] break-all text-muted">{info.logsDir}</code>
            )}
          </>
        }
      >
        <button type="button" className="btn btn-line btn-sm" data-testid="open-logs" onClick={openLogs}>
          <Icon name="folder" size={18} />
          {t.openLogs}
        </button>
      </SetRow>
      <SetRow stack title={t.legal} help={t.legalHelp} />
      <SetRow stack title={t.creditsTitle}>
        <ul className="m-0 grid list-none gap-1.5 p-0 text-[14px] text-muted">
          {CREDITS.map((c) => (
            <li key={c.name}>
              <button type="button" className={linkBtn} onClick={() => open(c.url)}>
                {c.name}
              </button>
              : {t.credits[c.role]}
            </li>
          ))}
        </ul>
      </SetRow>
    </SetSection>
  );
}
