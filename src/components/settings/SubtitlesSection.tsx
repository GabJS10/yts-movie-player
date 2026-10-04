import { useRef, useState, type ReactNode } from "react";
import { describeError } from "../../api/errors";
import { useUpdateSettings } from "../../api/queries";
import { openExternalUrl, toAppError } from "../../api/tauri";
import type { AppError, Settings, SettingsPatch, SubtitlesStatus } from "../../api/types";
import { formatResetTime, OPENSUBTITLES_KEYS_URL, SUBTITLE_LANGS } from "../../lib/subtitles";
import { fetchSubtitlesStatus } from "../../store/subtitlesQuota";
import { Icon } from "../Icon";
import { SetRow, SetSection, Switch } from "./controls";

/**
 * Text setting saved on blur or Enter; empty clears it (null). Re-seeded when the stored value
 * changes elsewhere. `secret` hides it behind a Mostrar/Ocultar toggle.
 */
function CommitInput({
  value,
  onCommit,
  label,
  secret = false,
  placeholder,
  autoComplete = "off",
}: {
  value: string | null;
  onCommit: (next: string | null) => void;
  label: string;
  secret?: boolean;
  placeholder?: string;
  autoComplete?: string;
}) {
  const [draft, setDraft] = useState(value ?? "");
  const [seen, setSeen] = useState(value);
  const [visible, setVisible] = useState(false);
  if (seen !== value) {
    setSeen(value);
    setDraft(value ?? "");
  }
  const commit = () => {
    const next = draft.trim() || null;
    if (next !== value) onCommit(next);
  };
  return (
    <div className="flex gap-2 max-[640px]:flex-wrap">
      <input
        className="input input-mono min-w-0 flex-1"
        type={secret && !visible ? "password" : "text"}
        aria-label={label}
        value={draft}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete={autoComplete}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          if (e.key === "Escape") setDraft(value ?? "");
        }}
      />
      {secret && (
        <button
          type="button"
          className="btn btn-line btn-sm"
          aria-pressed={visible}
          aria-label={visible ? `Ocultar ${label.toLowerCase()}` : `Mostrar ${label.toLowerCase()}`}
          onClick={() => setVisible((v) => !v)}
        >
          <Icon name="eye" size={18} />
          {visible ? "Ocultar" : "Mostrar"}
        </button>
      )}
    </div>
  );
}

type TestResult = { ok: true; status: SubtitlesStatus } | { ok: false; error: AppError };

function describeStatus(s: SubtitlesStatus, hasAccount: boolean): string {
  const parts = ["Clave válida"];
  if (s.loggedIn) parts.push("sesión iniciada");
  else if (hasAccount) parts.push("no se pudo iniciar sesión: revisa el usuario y la contraseña");
  if (s.remainingDownloads !== null) {
    const at = formatResetTime(s.resetAt);
    parts.push(
      `${s.remainingDownloads === 1 ? "queda 1 descarga" : `quedan ${s.remainingDownloads} descargas`} hoy${at ? ` (se renueva a las ${at})` : ""}`,
    );
  }
  return parts.join(" · ");
}

const openKeysPage = () => void openExternalUrl(OPENSUBTITLES_KEYS_URL).catch(() => undefined);

/** Ajustes › Subtítulos: OpenSubtitles key and optional account, "Probar", language and auto-load. */
export function SubtitlesSection({ settings }: { settings: Settings }) {
  const update = useUpdateSettings();
  const [result, setResult] = useState<TestResult | null>(null);
  const [testing, setTesting] = useState(false);
  // "Probar" right after editing a field (blur → save) must test the saved values.
  const saving = useRef<Promise<unknown>>(Promise.resolve());

  const save = (patch: SettingsPatch) => {
    setResult(null);
    saving.current = update.mutateAsync(patch).catch(() => undefined);
  };

  const test = async () => {
    setTesting(true);
    await saving.current;
    try {
      setResult({ ok: true, status: await fetchSubtitlesStatus() });
    } catch (err) {
      setResult({ ok: false, error: toAppError(err) });
    } finally {
      setTesting(false);
    }
  };

  const hasKey = !!settings.openSubtitlesApiKey;
  const hasAccount = !!(settings.openSubtitlesUsername && settings.openSubtitlesPassword);
  let resultText: ReactNode = null;
  if (result?.ok)
    resultText = <span className="text-green">{describeStatus(result.status, hasAccount)}</span>;
  else if (result)
    resultText = (
      <span className="text-danger">
        {result.error.code === "subtitles_auth"
          ? "OpenSubtitles no acepta esta clave. Revisa que esté completa."
          : describeError(result.error).title}
      </span>
    );

  return (
    <SetSection
      id="s-subs"
      title="Subtítulos"
      lede="Se buscan en OpenSubtitles por el código IMDb de la película y se eligen los que mejor encajan con la versión de YTS."
    >
      {!hasKey && (
        <div
          className="mb-2 grid gap-1.5 rounded-lg border border-line bg-surface px-4 py-3 text-sm"
          role="note"
        >
          <b className="font-bold">Falta la clave de OpenSubtitles</b>
          <span className="text-text-2">
            Sin ella no se buscan subtítulos; solo puedes cargar tus propios archivos .srt desde el
            reproductor. Es gratis: crea una cuenta en opensubtitles.com, entra en tu perfil › API consumers y
            crea una clave.
          </span>
          <button
            type="button"
            className="justify-self-start font-semibold text-green hover:underline"
            onClick={openKeysPage}
          >
            Conseguir una clave en opensubtitles.com
          </button>
        </div>
      )}
      <SetRow
        stack
        title="Clave de API de OpenSubtitles"
        help="Gratis en opensubtitles.com › Perfil › API consumers."
      >
        <CommitInput
          label="Clave de API"
          secret
          value={settings.openSubtitlesApiKey}
          placeholder="Pega aquí tu clave"
          onCommit={(v) => save({ openSubtitlesApiKey: v })}
        />
      </SetRow>
      <SetRow
        stack
        title="Cuenta de OpenSubtitles (opcional)"
        help="Con una cuenta gratuita el cupo diario de descargas es mayor."
      >
        <div className="grid grid-cols-2 gap-2 max-[640px]:grid-cols-1">
          <CommitInput
            label="Usuario"
            value={settings.openSubtitlesUsername}
            placeholder="Usuario"
            autoComplete="username"
            onCommit={(v) => save({ openSubtitlesUsername: v })}
          />
          <CommitInput
            label="Contraseña"
            secret
            value={settings.openSubtitlesPassword}
            placeholder="Contraseña"
            autoComplete="current-password"
            onCommit={(v) => save({ openSubtitlesPassword: v })}
          />
        </div>
      </SetRow>
      <SetRow
        title="Probar la conexión"
        help={resultText ?? "Comprueba la clave, la sesión y el cupo que queda hoy."}
      >
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={!hasKey || testing}
          title={hasKey ? undefined : "Primero añade la clave"}
          onClick={() => void test()}
        >
          <Icon name="refresh" size={18} />
          {testing ? "Probando…" : "Probar"}
        </button>
      </SetRow>
      <SetRow title="Idioma preferido" id="subs-lang-title">
        <select
          className="select"
          aria-labelledby="subs-lang-title"
          value={settings.subtitleLang}
          onChange={(e) => save({ subtitleLang: e.target.value })}
        >
          {SUBTITLE_LANGS.map((l) => (
            <option key={l.code} value={l.code}>
              {l.label}
            </option>
          ))}
        </select>
      </SetRow>
      <SetRow
        title="Buscar subtítulos automáticamente"
        help="Al empezar una película se cargan en tu idioma sin pedírtelo."
      >
        <Switch
          label="Buscar subtítulos automáticamente"
          checked={settings.autoSubtitles}
          onChange={(autoSubtitles) => save({ autoSubtitles })}
        />
      </SetRow>
    </SetSection>
  );
}
