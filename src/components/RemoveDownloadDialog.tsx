import { useEffect, useRef, type KeyboardEvent } from "react";
import type { Download } from "../api/types";
import { formatBytes } from "../lib/format";
import { Icon } from "./Icon";

type Props = {
  download: Download;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (deleteFiles: boolean) => void;
};

/** "Quitar": keep the files in the library folder, or delete them too. Esc or the backdrop cancels. */
export function RemoveDownloadDialog({ download, busy, onCancel, onConfirm }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);

  // Focus moves in on open and goes back to whatever opened it on close.
  useEffect(() => {
    const opener = document.activeElement as HTMLElement | null;
    cancel.current?.focus();
    return () => opener?.focus?.();
  }, []);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onCancel();
      return;
    }
    if (e.key !== "Tab") return;
    const items = [...(box.current?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? [])];
    const first = items[0];
    const last = items.at(-1);
    if (!first || !last) return;
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  const { movie } = download;
  const done = download.state === "done";
  const bytes = done ? download.sizeBytes : download.downloadedBytes;

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onCancel()}>
      <div
        ref={box}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="remove-dl-title"
        aria-describedby="remove-dl-desc"
        onKeyDown={onKeyDown}
      >
        <h2 id="remove-dl-title" className="m-0 mb-2 text-[20px] font-extrabold text-balance">
          ¿Quitar {movie.title} ({download.quality})?
        </h2>
        <p id="remove-dl-desc" className="m-0 mb-6 text-[14.5px] text-muted">
          ¿Borrar también los archivos? Liberarías <b className="text-text-2 tnum">{formatBytes(bytes)}</b>
          {done ? " y dejará de verse sin conexión" : ""}. Si los conservas, se quedan en{" "}
          {download.path ? <code className="text-text-2 break-all">{download.path}</code> : "su carpeta"}.
        </p>
        <div className="flex flex-wrap justify-end gap-2.5">
          <button ref={cancel} type="button" className="btn btn-line btn-sm" onClick={onCancel}>
            Cancelar
          </button>
          <button
            type="button"
            className="btn btn-line btn-sm"
            disabled={busy}
            onClick={() => onConfirm(false)}
          >
            Conservar archivos
          </button>
          <button
            type="button"
            className="btn btn-danger btn-sm"
            disabled={busy}
            onClick={() => onConfirm(true)}
          >
            <Icon name="trash" size={18} />
            Borrar archivos
          </button>
        </div>
      </div>
    </div>
  );
}
