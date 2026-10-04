import { useEffect, useRef, useState, type ReactNode } from "react";
import { useClearCache, useMoveDownloads, useStorageUsage, useUpdateSettings } from "../../api/queries";
import { pickFolder, toAppError } from "../../api/tauri";
import type { Settings, StorageUsage } from "../../api/types";
import { formatBytes } from "../../lib/format";
import { isDefaultFolder } from "../../lib/settings";
import { moveRunning, useMoveStore } from "../../store/moveDownloads";
import { showToast } from "../../store/toast";
import { Icon } from "../Icon";
import { SetRow, SetSection } from "./controls";
import { MoveDownloadsDialog } from "./MoveDownloadsDialog";

const GB = 1024 ** 3;
export const CACHE_MIN_GB = 2;
export const CACHE_MAX_GB = 100;
/** The slider saves once it stops moving, not on every step. */
export const CACHE_COMMIT_MS = 500;

export function Usage({
  label,
  value,
  note,
  children,
}: {
  label: string;
  value: ReactNode;
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

type FolderKey = "downloadsDir" | "cacheDir";

const FOLDERS: Record<
  FolderKey,
  {
    title: string;
    help: string;
    missing: string;
    used: (u: StorageUsage) => number;
    free: (u: StorageUsage) => number;
    available: (u: StorageUsage) => boolean;
  }
> = {
  downloadsDir: {
    title: "Carpeta de descargas",
    help: "Las descargas nuevas se guardan aquí. Las que ya tienes se quedan donde están hasta que las muevas.",
    missing:
      "No encontramos esta carpeta (¿un disco desconectado?). Las descargas que viven ahí aparecen como «Carpeta no disponible» y siguen solas cuando vuelva.",
    used: (u) => u.libraryBytes,
    free: (u) => u.downloadsFreeBytes,
    available: (u) => u.downloadsDirAvailable,
  },
  cacheDir: {
    title: "Carpeta de la caché de streaming",
    help: "Lo que ves sin descargar. Al cambiarla se vacía la caché actual; no toca tus descargas.",
    missing:
      "No encontramos esta carpeta (¿un disco desconectado?). Mientras tanto, el streaming usa la carpeta por defecto.",
    used: (u) => u.cacheBytes,
    free: (u) => u.cacheFreeBytes,
    available: (u) => u.cacheDirAvailable,
  },
};

/** A data folder: path, the system picker, Restablecer, its disk's free space and what it holds. */
function FolderRow({
  field,
  settings,
  usage,
}: {
  field: FolderKey;
  settings: Settings;
  usage: StorageUsage | undefined;
}) {
  const update = useUpdateSettings();
  const meta = FOLDERS[field];
  const path = settings[field];
  const id = `${field}-title`;

  const change = async () => {
    let picked: string | null;
    try {
      picked = await pickFolder(meta.title, path);
    } catch (err) {
      console.warn("folder picker failed", toAppError(err).message);
      showToast("No se pudo abrir el selector de carpetas", "error");
      return;
    }
    if (picked && picked !== path) update.mutate({ [field]: picked });
  };

  return (
    <SetRow stack title={meta.title} help={meta.help} id={id}>
      <div className="flex gap-2 max-[640px]:flex-wrap">
        <input className="input input-mono min-w-0 flex-1" aria-labelledby={id} value={path} readOnly />
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={update.isPending}
          aria-label={`Cambiar la ${meta.title.toLowerCase()}`}
          onClick={() => void change()}
        >
          <Icon name="folder" size={18} />
          Cambiar…
        </button>
        {!isDefaultFolder(field, path) && (
          <button
            type="button"
            className="btn btn-line btn-sm"
            disabled={update.isPending}
            aria-label={`Restablecer la ${meta.title.toLowerCase()}`}
            onClick={() => update.mutate({ [field]: null })}
          >
            <Icon name="refresh" size={18} />
            Restablecer
          </button>
        )}
      </div>
      {usage && (
        <p className="!mt-0 tnum">
          Libre en ese disco: {formatBytes(meta.free(usage))} · Ocupa: {formatBytes(meta.used(usage))}
        </p>
      )}
      {usage && !meta.available(usage) && (
        <p className="!mt-0 flex items-start gap-2 text-warn" role="status">
          <Icon name="alert" size={16} className="mt-0.5 flex-none" />
          {meta.missing}
        </p>
      )}
    </SetRow>
  );
}

/** "Mover también las descargas existentes" once downloadsDir changes, with its progress dialog. */
function MoveOffer({ usage }: { usage: StorageUsage | undefined }) {
  const move = useMoveDownloads();
  const running = useMoveStore(moveRunning);
  const open = useMoveStore((s) => s.open);
  const outside = usage?.downloadsOutsideDir ?? 0;

  return (
    <>
      {(outside > 0 || running) && (
        <SetRow
          title="Descargas en otra carpeta"
          help={
            running
              ? "Moviendo las descargas a la carpeta nueva…"
              : `${outside === 1 ? "1 descarga sigue" : `${outside} descargas siguen`} en una carpeta anterior. Se ven igual; muévelas si quieres tenerlo todo junto.`
          }
        >
          {running ? (
            <button
              type="button"
              className="btn btn-line btn-sm"
              onClick={() => useMoveStore.setState({ open: true })}
            >
              Ver progreso
            </button>
          ) : (
            <button
              type="button"
              className="btn btn-line btn-sm"
              disabled={move.isPending}
              onClick={() => move.mutate()}
            >
              Mover también las descargas existentes
            </button>
          )}
        </SetRow>
      )}
      {open && <MoveDownloadsDialog />}
    </>
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
        className="m-0 mb-2 grid grid-cols-2 border-y border-line max-[640px]:grid-cols-1"
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

      <FolderRow field="downloadsDir" settings={settings} usage={u} />
      <MoveOffer usage={u} />
      <FolderRow field="cacheDir" settings={settings} usage={u} />

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
