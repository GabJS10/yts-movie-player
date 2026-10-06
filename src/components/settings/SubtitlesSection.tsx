import { useRef, useState, type ReactNode } from "react";
import { describeError } from "../../api/errors";
import { useUpdateSettings } from "../../api/queries";
import { openExternalUrl, toAppError } from "../../api/tauri";
import type { AppError, Settings, SettingsPatch, SubtitlesStatus } from "../../api/types";
import { getT, useT } from "../../i18n";
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
  const t = useT().subtitlesSettings;
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
          aria-label={visible ? t.hideField(label) : t.showField(label)}
          onClick={() => setVisible((v) => !v)}
        >
          <Icon name="eye" size={18} />
          {visible ? t.hide : t.show}
        </button>
      )}
    </div>
  );
}

type TestResult = { ok: true; status: SubtitlesStatus } | { ok: false; error: AppError };

function describeStatus(s: SubtitlesStatus, hasAccount: boolean): string {
  const t = getT().subtitlesSettings;
  const parts = [t.validKey];
  if (s.loggedIn) parts.push(t.loggedIn);
  else if (hasAccount) parts.push(t.loginFailed);
  if (s.remainingDownloads !== null)
    parts.push(t.remaining(s.remainingDownloads, formatResetTime(s.resetAt)));
  return parts.join(" · ");
}

const openKeysPage = () => void openExternalUrl(OPENSUBTITLES_KEYS_URL).catch(() => undefined);

/** Ajustes › Subtítulos: OpenSubtitles key and optional account, "Probar", language and auto-load. */
export function SubtitlesSection({ settings }: { settings: Settings }) {
  const update = useUpdateSettings();
  const t = useT().subtitlesSettings;
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
        {result.error.code === "subtitles_auth" ? t.keyRejected : describeError(result.error).title}
      </span>
    );

  return (
    <SetSection id="s-subs" title={t.title} lede={t.lede}>
      {!hasKey && (
        <div
          className="mb-2 grid gap-1.5 rounded-lg border border-line bg-surface px-4 py-3 text-sm"
          role="note"
        >
          <b className="font-bold">{t.missingKey}</b>
          <span className="text-text-2">{t.missingKeyHelp}</span>
          <button
            type="button"
            className="justify-self-start font-semibold text-green hover:underline"
            onClick={openKeysPage}
          >
            {t.getKey}
          </button>
        </div>
      )}
      <SetRow stack title={t.apiKey} help={t.apiKeyHelp}>
        <CommitInput
          label={t.apiKeyLabel}
          secret
          value={settings.openSubtitlesApiKey}
          placeholder={t.apiKeyPlaceholder}
          onCommit={(v) => save({ openSubtitlesApiKey: v })}
        />
      </SetRow>
      <SetRow stack title={t.account} help={t.accountHelp}>
        <div className="grid grid-cols-2 gap-2 max-[640px]:grid-cols-1">
          <CommitInput
            label={t.username}
            value={settings.openSubtitlesUsername}
            placeholder={t.username}
            autoComplete="username"
            onCommit={(v) => save({ openSubtitlesUsername: v })}
          />
          <CommitInput
            label={t.password}
            secret
            value={settings.openSubtitlesPassword}
            placeholder={t.password}
            autoComplete="current-password"
            onCommit={(v) => save({ openSubtitlesPassword: v })}
          />
        </div>
      </SetRow>
      <SetRow title={t.test} help={resultText ?? t.testHelp}>
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={!hasKey || testing}
          title={hasKey ? undefined : t.addKeyFirst}
          onClick={() => void test()}
        >
          <Icon name="refresh" size={18} />
          {testing ? t.testing : t.testShort}
        </button>
      </SetRow>
      <SetRow title={t.language} id="subs-lang-title">
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
      <SetRow title={t.auto} help={t.autoHelp}>
        <Switch
          label={t.auto}
          checked={settings.autoSubtitles}
          onChange={(autoSubtitles) => save({ autoSubtitles })}
        />
      </SetRow>
    </SetSection>
  );
}
