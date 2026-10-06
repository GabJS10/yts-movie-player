import { useState } from "react";
import { useUpdateSettings } from "../../api/queries";
import type { Settings } from "../../api/types";
import { useT } from "../../i18n";
import { parseOptionalInt } from "../../lib/settings";
import { SetRow, SetSection, Switch } from "./controls";

export const PORT_MIN = 1024;
export const PORT_MAX = 65535;

type NumberKey = "downLimitKbps" | "upLimitKbps" | "listenPort";

/** A number saved on blur or Enter; Esc goes back to the stored value. */
function OptionalNumber({
  field,
  label,
  value,
  min,
  max,
  unit,
  onSave,
}: {
  field: NumberKey;
  label: string;
  value: number | null;
  min: number;
  max: number;
  unit?: string;
  /** `onError` puts the stored value back (the optimistic update and its rollback may batch together). */
  onSave: (field: NumberKey, value: number | null, onError: () => void) => void;
}) {
  const t = useT().torrent;
  const stored = value === null ? "" : String(value);
  const [draft, setDraft] = useState(stored);
  const [seen, setSeen] = useState(stored);
  if (seen !== stored) {
    setSeen(stored);
    setDraft(stored);
  }
  const parsed = parseOptionalInt(draft, min, max);
  const error = typeof parsed === "string" ? parsed : null;

  const commit = () => {
    if (typeof parsed === "string") return;
    if (parsed !== value) onSave(field, parsed, () => setDraft(stored));
    else setDraft(stored);
  };

  return (
    <div className="grid justify-items-end gap-1">
      <label className="inline-flex items-center gap-2 text-[13px] text-muted">
        <input
          className={`input text-right tnum ${unit ? "w-24" : "w-[110px]"}`}
          inputMode="numeric"
          aria-label={label}
          aria-invalid={error ? true : undefined}
          placeholder={unit ? t.noLimit : t.auto}
          value={draft}
          autoComplete="off"
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") setDraft(stored);
          }}
        />
        {unit}
      </label>
      {error && <span className="text-[12.5px] text-danger">{error}</span>}
    </div>
  );
}

/** Ajustes › Torrent: speed limits and seeding apply at once; the port, after restarting the app. */
export function TorrentSection({ settings }: { settings: Settings }) {
  const update = useUpdateSettings();
  const t = useT().torrent;
  const save = (field: NumberKey, value: number | null, onError: () => void) =>
    update.mutate({ [field]: value }, { onError });

  return (
    <SetSection id="s-torrent" title="Torrent">
      <SetRow title={t.downLimit} help={t.limitHelp}>
        <OptionalNumber
          field="downLimitKbps"
          label={t.downLimit}
          value={settings.downLimitKbps}
          min={1}
          max={10_000_000}
          unit="KB/s"
          onSave={save}
        />
      </SetRow>
      <SetRow title={t.upLimit} help={t.limitHelp}>
        <OptionalNumber
          field="upLimitKbps"
          label={t.upLimit}
          value={settings.upLimitKbps}
          min={1}
          max={10_000_000}
          unit="KB/s"
          onSave={save}
        />
      </SetRow>
      <SetRow title={t.seed} help={t.seedHelp}>
        <Switch
          label={t.seed}
          checked={settings.seedAfterDownload}
          onChange={(seedAfterDownload) => update.mutate({ seedAfterDownload })}
        />
      </SetRow>
      <SetRow title={t.port} help={t.portHelp(PORT_MIN, PORT_MAX)}>
        <OptionalNumber
          field="listenPort"
          label={t.port}
          value={settings.listenPort}
          min={PORT_MIN}
          max={PORT_MAX}
          onSave={save}
        />
      </SetRow>
    </SetSection>
  );
}
