import { useEffect, useRef, useState, type ReactNode } from "react";
import { useClearCache, useStorageUsage, useUpdateSettings } from "../../api/queries";
import type { Settings } from "../../api/types";
import { formatBytes } from "../../lib/format";
import { Icon } from "../Icon";
import { SetRow, SetSection } from "./controls";

const GB = 1024 ** 3;
export const CACHE_MIN_GB = 2;
export const CACHE_MAX_GB = 100;
/** The slider saves once it stops moving, not on every step. */
export const CACHE_COMMIT_MS = 500;

function Usage({
  label,
  value,
  note,
  children,
}: {
  label: string;
  value: string;
  note?: string;
  children?: ReactNode;
}) {
  return (
    <div className="py-3.5 pr-6 [&+div]:border-l [&+div]:border-line [&+div]:pl-6 max-[640px]:[&+div]:border-t max-[640px]:[&+div]:border-l-0 max-[640px]:[&+div]:pl-0">
      <dt className="field-label">{label}</dt>
      <dd className="m-0 mt-1 mb-2.5 text-base font-bold tnum">{value}</dd>
      {children}
      {note && <p className="m-0 mt-2 text-[12.5px] text-muted">{note}</p>}
    </div>
  );
}

/** Ajustes › Almacenamiento: cache use vs. its limit, the limit itself and "Vaciar caché ahora". */
export function StorageSection({ settings }: { settings: Settings }) {
  const usage = useStorageUsage();
  const update = useUpdateSettings();
  const clear = useClearCache();

  const savedGb = Math.round(settings.cacheLimitBytes / GB);
  const [draftGb, setDraftGb] = useState<number | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const pending = useRef<number | null>(null);
  const mutate = update.mutate;

  const commit = () => {
    window.clearTimeout(timer.current);
    const gb = pending.current;
    pending.current = null;
    if (gb === null) return;
    setDraftGb(null);
    if (gb * GB !== settings.cacheLimitBytes) mutate({ cacheLimitBytes: gb * GB });
  };
  const commitRef = useRef(commit);
  useEffect(() => {
    commitRef.current = commit;
  });
  // Leaving the page mid-drag still saves the last value.
  useEffect(() => () => commitRef.current(), []);

  const onLimit = (gb: number) => {
    setDraftGb(gb);
    pending.current = gb;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => commitRef.current(), CACHE_COMMIT_MS);
  };

  const limitGb = draftGb ?? savedGb;
  const u = usage.data;
  const cacheBytes = u?.cacheBytes ?? null;
  const shrinking = cacheBytes !== null && limitGb * GB < cacheBytes;
  const fill = u && u.cacheLimitBytes > 0 ? Math.min(1, u.cacheBytes / u.cacheLimitBytes) : 0;

  return (
    <SetSection id="s-disk" title="Almacenamiento">
      <dl
        className="m-0 mb-2 grid grid-cols-3 border-y border-line max-[640px]:grid-cols-1"
        aria-busy={usage.isPending || undefined}
      >
        <Usage
          label="Caché de streaming"
          value={u ? `${formatBytes(u.cacheBytes)} de ${formatBytes(u.cacheLimitBytes)}` : "—"}
          note="Se vacía sola, empezando por lo menos usado"
        >
          <div
            className={`meter ${u && u.cacheBytes > u.cacheLimitBytes ? "meter-over" : ""}`}
            role="meter"
            aria-label="Uso de la caché"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(fill * 100)}
          >
            <i style={{ width: `${(fill * 100).toFixed(1)}%` }} />
          </div>
        </Usage>
        <Usage label="Biblioteca" value={u ? formatBytes(u.libraryBytes) : "—"} note="Descargas guardadas" />
        <Usage label="Libre en disco" value={u ? formatBytes(u.freeDiskBytes) : "—"} />
      </dl>
      {usage.isError && (
        <p className="m-0 mb-2 text-[13.5px] text-muted" role="status">
          No se pudo medir el espacio.{" "}
          <button
            type="button"
            className="font-semibold text-green hover:underline"
            onClick={() => void usage.refetch()}
          >
            Reintentar
          </button>
        </p>
      )}

      <SetRow
        stack
        title="Carpeta de datos"
        help="Cambiarla llegará en una próxima versión."
        id="data-dir-title"
      >
        <input
          className="input input-mono"
          aria-labelledby="data-dir-title"
          value={settings.dataDir}
          readOnly
        />
      </SetRow>

      <SetRow
        stack
        title="Límite de la caché de streaming"
        help="Lo que ves sin descargar ocupa espacio temporal. Al pasar el límite se borra lo menos usado; nunca lo que estás viendo ni tu biblioteca."
        id="cache-limit-title"
      >
        <div className="flex items-center gap-4">
          <input
            className="range"
            type="range"
            min={CACHE_MIN_GB}
            max={Math.max(CACHE_MAX_GB, savedGb)}
            step={1}
            value={limitGb}
            aria-labelledby="cache-limit-title"
            aria-valuetext={`${limitGb} GB`}
            onChange={(e) => onLimit(Number(e.target.value))}
            onPointerUp={() => commitRef.current()}
          />
          <output className="min-w-16 text-right font-extrabold tnum" aria-live="polite">
            {limitGb} GB
          </output>
        </div>
        {shrinking && (
          <p className="!mt-0 text-warn">
            Ahora hay {formatBytes(cacheBytes)} en caché: se borrará lo menos usado hasta bajar de {limitGb}{" "}
            GB.
          </p>
        )}
      </SetRow>

      <SetRow
        title="Vaciar caché ahora"
        help={
          <span className="tnum">
            {u ? `${formatBytes(u.cacheBytes)} en uso. ` : ""}No toca las descargas de tu biblioteca ni lo que
            se está reproduciendo.
          </span>
        }
      >
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={clear.isPending || u?.cacheBytes === 0}
          onClick={() => clear.mutate()}
        >
          <Icon name="trash" size={18} />
          {clear.isPending ? "Vaciando…" : "Vaciar caché"}
        </button>
      </SetRow>
    </SetSection>
  );
}
