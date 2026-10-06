import type { Download } from "../api/types";
import { useT } from "../i18n";
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
  const t = useT().removeDialog;
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
        {t.title(movie.title, download.quality)}
      </h2>
      <p id="remove-dl-desc" className="m-0 mb-6 text-[14.5px] text-muted">
        {t.free}
        <b className="text-text-2 tnum">{formatBytes(bytes)}</b>
        {done ? t.noLongerOffline : ""}
        {t.keepIn}
        {download.path ? <code className="break-all text-text-2">{download.path}</code> : t.theirFolder}.
      </p>
      <div className="flex flex-wrap justify-end gap-2.5">
        <button
          type="button"
          className="btn btn-line btn-sm"
          data-autofocus
          data-testid="remove-cancel"
          onClick={onCancel}
        >
          {t.cancel}
        </button>
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={busy}
          data-testid="remove-keep"
          onClick={() => onConfirm(false)}
        >
          {t.keep}
        </button>
        <button
          type="button"
          className="btn btn-danger btn-sm"
          disabled={busy}
          data-testid="remove-delete"
          onClick={() => onConfirm(true)}
        >
          <Icon name="trash" size={18} />
          {t.delete}
        </button>
      </div>
    </Modal>
  );
}
