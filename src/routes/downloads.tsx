import { createFileRoute, Link } from "@tanstack/react-router";
import { useState } from "react";
import {
  useDownloads,
  useOpenDownloadFolder,
  useRemoveDownload,
  useSettings,
  useStorageUsage,
  useToggleDownload,
} from "../api/queries";
import type { Download } from "../api/types";
import { ErrorState } from "../components/ErrorState";
import { Icon } from "../components/Icon";
import { Poster } from "../components/Poster";
import { RemoveDownloadDialog } from "../components/RemoveDownloadDialog";
import { Usage } from "../components/settings/StorageSection";
import {
  canPause,
  formatEta,
  formatPercent,
  groupDownloads,
  isActive,
  isComplete,
  isLocked,
  STATE_LABEL,
} from "../lib/downloads";
import { formatBytes, formatSpeed } from "../lib/format";

export const Route = createFileRoute("/downloads")({ component: DownloadsPage });

function DownloadsPage() {
  // Progress comes from polling list_downloads every second while this page is open.
  const downloads = useDownloads({ poll: true });
  const [removing, setRemoving] = useState<Download | null>(null);
  const remove = useRemoveDownload();
  const list = downloads.data;
  const { inProgress, library } = groupDownloads(list ?? []);

  return (
    <div className="min-h-screen px-gutter pt-[calc(var(--spacing-nav)+36px)] pb-24">
      <h1 className="m-0 mb-1.5 text-headline font-[850] uppercase stretch-condensed">Descargas</h1>
      <p className="m-0 mb-8 text-[15px] text-muted">
        Lo que descargas queda guardado en tu biblioteca y se puede ver sin conexión.
      </p>
      <StorageSummary downloads={list ?? []} />

      {downloads.isError && !list ? (
        <ErrorState error={downloads.error} onRetry={() => void downloads.refetch()} />
      ) : !list ? (
        <div aria-busy="true" aria-label="Cargando descargas">
          {[0, 1, 2].map((i) => (
            <div key={i} className="skeleton mb-3 h-24" />
          ))}
        </div>
      ) : list.length === 0 ? (
        <div className="grid max-w-[460px] justify-items-start gap-3.5 py-16">
          <h2 className="m-0 text-[22px] font-extrabold">No hay descargas</h2>
          <p className="m-0 text-muted">
            Pulsa «Descargar» en una película para guardarla y verla sin conexión.
          </p>
          <Link to="/" className="btn btn-line btn-sm">
            Explorar el catálogo
          </Link>
        </div>
      ) : (
        <>
          {inProgress.length > 0 && (
            <DownloadGroup title="En curso" items={inProgress} onRemove={setRemoving} />
          )}
          {library.length > 0 && (
            <DownloadGroup title="En la biblioteca" items={library} onRemove={setRemoving} />
          )}
        </>
      )}

      {removing && (
        <RemoveDownloadDialog
          download={removing}
          busy={remove.isPending}
          onCancel={() => setRemoving(null)}
          onConfirm={(deleteFiles) =>
            remove.mutate({ download: removing, deleteFiles }, { onSettled: () => setRemoving(null) })
          }
        />
      )}
    </div>
  );
}

/** Library size, streaming cache and current speed, as in the prototype. */
function StorageSummary({ downloads }: { downloads: Download[] }) {
  const usage = useStorageUsage().data;
  const settings = useSettings().data;
  const libraryCount = downloads.filter((d) => d.state === "done").length;
  const speed = downloads.reduce((sum, d) => sum + (isActive(d) ? d.downSpeedBps : 0), 0);
  const fill = usage && usage.cacheLimitBytes > 0 ? Math.min(1, usage.cacheBytes / usage.cacheLimitBytes) : 0;

  return (
    <dl className="m-0 mb-9 grid grid-cols-3 border-y border-line max-[760px]:grid-cols-1">
      <Usage
        label="Biblioteca"
        value={
          usage
            ? `${formatBytes(usage.libraryBytes)} · ${libraryCount === 1 ? "1 película" : `${libraryCount} películas`}`
            : "—"
        }
        note={settings?.downloadsDir}
      />
      <Usage
        label="Caché de streaming"
        value={usage ? `${formatBytes(usage.cacheBytes)} de ${formatBytes(usage.cacheLimitBytes)}` : "—"}
        note="Se vacía sola, empezando por lo menos usado"
      >
        <div
          className="meter"
          role="meter"
          aria-label="Uso de la caché"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(fill * 100)}
        >
          <i style={{ width: `${(fill * 100).toFixed(1)}%` }} />
        </div>
      </Usage>
      <Usage
        label="Ahora"
        value={
          <span className="inline-flex items-center gap-1">
            <Icon name="down" size={16} />
            {formatSpeed(speed)}
          </span>
        }
        note={
          settings
            ? settings.seedAfterDownload
              ? "Se sigue compartiendo al terminar"
              : "No se comparte al terminar"
            : undefined
        }
      />
    </dl>
  );
}

function DownloadGroup({
  title,
  items,
  onRemove,
}: {
  title: string;
  items: Download[];
  onRemove: (d: Download) => void;
}) {
  const id = `dl-group-${title.replace(/\W+/g, "-").toLowerCase()}`;
  return (
    <section aria-labelledby={id} className="mb-10">
      <h2 id={id} className="field-label m-0 mb-1.5 text-[13px]">
        {title}
      </h2>
      <ul className="m-0 list-none p-0">
        {items.map((d) => (
          <DownloadRow key={d.infohash} download={d} onRemove={() => onRemove(d)} />
        ))}
      </ul>
    </section>
  );
}

function DownloadStatus({ d }: { d: Download }) {
  const amount = `${formatBytes(d.downloadedBytes)} de ${formatBytes(d.sizeBytes)}`;
  switch (d.state) {
    case "queued":
      return (
        <>
          <b>{STATE_LABEL.queued}</b>
          <span>{amount}</span>
          <span>Preparando la descarga…</span>
        </>
      );
    case "active":
      return (
        <>
          <b>{STATE_LABEL.active}</b>
          <span>{formatPercent(d.progress)}</span>
          <span>{amount}</span>
          <span className="inline-flex items-center gap-1">
            <Icon name="down" size={14} />
            {formatSpeed(d.downSpeedBps)}
          </span>
          <span>{d.peers === 1 ? "1 peer" : `${d.peers} peers`}</span>
          {d.etaS !== null && <span>{formatEta(d.etaS)}</span>}
        </>
      );
    case "paused":
      return (
        <>
          <b>{STATE_LABEL.paused}</b>
          <span>{formatPercent(d.progress)}</span>
          <span>{amount}</span>
        </>
      );
    case "stalled":
      return (
        <>
          <b>{STATE_LABEL.stalled}</b>
          <span>{formatPercent(d.progress)}</span>
          <span>Esperando a que alguien comparta el archivo completo. Prueba otra calidad.</span>
        </>
      );
    case "done":
      return (
        <>
          <b>{STATE_LABEL.done}</b>
          <span>{formatBytes(d.sizeBytes)}</span>
          <span>Se reproduce sin conexión desde la biblioteca</span>
        </>
      );
    case "error":
      return (
        <>
          <b>{STATE_LABEL.error}</b>
          <span>{formatPercent(d.progress)}</span>
          <span>Reanúdala, o quítala y vuelve a descargarla.</span>
        </>
      );
    case "unavailable":
      return (
        <>
          <b>{STATE_LABEL.unavailable}</b>
          <span>{isComplete(d) ? formatBytes(d.sizeBytes) : formatPercent(d.progress)}</span>
          <span>¿Un disco desconectado? Sigue sola cuando la carpeta vuelva.</span>
        </>
      );
    case "moving":
      return (
        <>
          <b>Moviendo a la carpeta nueva…</b>
          <span>{isComplete(d) ? formatBytes(d.sizeBytes) : formatPercent(d.progress)}</span>
        </>
      );
  }
}

function DownloadRow({ download: d, onRemove }: { download: Download; onRemove: () => void }) {
  const toggle = useToggleDownload();
  const folder = useOpenDownloadFolder();
  const { movie } = d;
  const name = `${movie.title} (${d.quality})`;
  const pause = canPause(d);
  const locked = isLocked(d);
  const lockedWhy =
    d.state === "moving" ? "Se está moviendo a la carpeta nueva" : "Su carpeta no está disponible";
  const percent = Math.round(d.progress * 100);

  return (
    <li
      data-testid="download-row"
      data-infohash={d.infohash}
      data-state={d.state}
      className={`dl-${d.state} grid grid-cols-[64px_minmax(0,1fr)_auto] items-center gap-5 border-b border-line py-4 max-[640px]:grid-cols-[48px_minmax(0,1fr)] max-[640px]:gap-3`}
    >
      <Link
        to="/movie/$movieId"
        params={{ movieId: movie.id }}
        tabIndex={-1}
        aria-hidden="true"
        className="block aspect-[2/3] overflow-hidden rounded-[3px]"
      >
        <Poster src={movie.coverUrl} title={movie.title} />
      </Link>
      <div className="min-w-0">
        <div className="mb-2.5 flex flex-wrap items-center gap-x-3.5 gap-y-2">
          <h3 className="m-0 text-[17px] font-[750]">
            <Link to="/movie/$movieId" params={{ movieId: movie.id }} className="hover:underline">
              {movie.title}
            </Link>{" "}
            <span className="font-medium text-muted">{movie.year}</span>
          </h3>
          <span className={`tag ${d.videoCodec === "x265" ? "tag-hevc" : ""}`}>
            <b>{d.quality}</b>
            <span className="codec">
              {d.videoCodec}
              {d.videoCodec === "x265" ? " · HEVC" : ""}
            </span>
            <span>{formatBytes(d.sizeBytes)}</span>
          </span>
        </div>
        <div
          className="dl-bar"
          role="progressbar"
          aria-label={`Progreso de ${movie.title}`}
          data-testid="download-progress"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
        >
          <i style={{ width: `${(d.progress * 100).toFixed(1)}%` }} />
        </div>
        <div className="dl-status">
          <DownloadStatus d={d} />
        </div>
      </div>
      <div className="flex gap-1 max-[640px]:col-start-2">
        {isComplete(d) && locked ? (
          <button
            type="button"
            className="dl-action"
            aria-label={`Reproducir ${name}`}
            title={lockedWhy}
            data-testid="download-play"
            disabled
          >
            <Icon name="play" />
          </button>
        ) : d.state === "done" ? (
          <Link
            to="/play/$movieId"
            params={{ movieId: movie.id }}
            search={{ infohash: d.infohash }}
            className="dl-action"
            aria-label={`Reproducir ${name}`}
            title="Reproducir"
            data-testid="download-play"
          >
            <Icon name="play" />
          </Link>
        ) : (
          <button
            type="button"
            className="dl-action"
            aria-label={`${pause ? "Pausar" : "Reanudar"} ${name}`}
            data-testid="download-toggle"
            title={locked ? lockedWhy : pause ? "Pausar" : "Reanudar"}
            disabled={locked || toggle.isPending}
            onClick={() => toggle.mutate({ infohash: d.infohash, pause })}
          >
            <Icon name={pause ? "pause" : "play"} />
          </button>
        )}
        <button
          type="button"
          className="dl-action"
          aria-label={`Abrir la carpeta de ${name}`}
          data-testid="download-folder"
          title={
            locked ? lockedWhy : d.path ? "Abrir carpeta" : "La carpeta se crea cuando empiece a descargarse"
          }
          disabled={locked || !d.path}
          onClick={() => folder.mutate(d.infohash)}
        >
          <Icon name="folder" />
        </button>
        <button
          type="button"
          className="dl-action"
          aria-label={`Quitar ${name}`}
          data-testid="download-remove"
          title={d.state === "moving" ? lockedWhy : "Quitar"}
          disabled={d.state === "moving"}
          onClick={onRemove}
        >
          <Icon name="trash" />
        </button>
      </div>
    </li>
  );
}
