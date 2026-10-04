import type { Download } from "../api/types";
import { formatBytes } from "../lib/format";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

type Props = {
  download: Download;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (deleteFiles: boolean) => void;
};

/** "Quitar": keep the files in the library folder, or delete them too. Esc or the backdrop cancels. */
export function RemoveDownloadDialog({ download, busy, onCancel, onConfirm }: Props) {
  const { movie } = download;
  const done = download.state === "done";
  const bytes = done ? download.sizeBytes : download.downloadedBytes;

  return (
    <Modal
      testId="remove-dialog"
      labelledBy="remove-dl-title"
      describedBy="remove-dl-desc"
      onClose={onCancel}
    >
      <h2 id="remove-dl-title" className="m-0 mb-2 text-[20px] font-extrabold text-balance">
        ¿Quitar {movie.title} ({download.quality})?
      </h2>
      <p id="remove-dl-desc" className="m-0 mb-6 text-[14.5px] text-muted">
        ¿Borrar también los archivos? Liberarías <b className="text-text-2 tnum">{formatBytes(bytes)}</b>
        {done ? " y dejará de verse sin conexión" : ""}. Si los conservas, se quedan en{" "}
        {download.path ? <code className="break-all text-text-2">{download.path}</code> : "su carpeta"}.
      </p>
      <div className="flex flex-wrap justify-end gap-2.5">
        <button
          type="button"
          className="btn btn-line btn-sm"
          data-autofocus
          data-testid="remove-cancel"
          onClick={onCancel}
        >
          Cancelar
        </button>
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={busy}
          data-testid="remove-keep"
          onClick={() => onConfirm(false)}
        >
          Conservar archivos
        </button>
        <button
          type="button"
          className="btn btn-danger btn-sm"
          disabled={busy}
          data-testid="remove-delete"
          onClick={() => onConfirm(true)}
        >
          <Icon name="trash" size={18} />
          Borrar archivos
        </button>
      </div>
    </Modal>
  );
}
