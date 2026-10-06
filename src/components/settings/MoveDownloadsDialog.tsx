import { useCancelMove, useDownloads } from "../../api/queries";
import { useT } from "../../i18n";
import { formatBytes } from "../../lib/format";
import { resetMove, useMoveStore } from "../../store/moveDownloads";
import { Modal } from "../Modal";

/**
 * Progress of move_downloads (downloads://move-progress): which one, how much, Cancelar; at the end, a
 * summary with the ones that couldn't move. While it runs, closing only hides it (the move goes on).
 */
export function MoveDownloadsDialog() {
  const { progress, starting } = useMoveStore();
  const cancel = useCancelMove();
  const t = useT().move;
  const downloads = useDownloads().data ?? [];
  const titleOf = (infohash: string | null) => {
    const d = downloads.find((x) => x.infohash === infohash);
    return d ? `${d.movie.title} (${d.quality})` : t.aDownload;
  };

  const finished = !!progress?.finished;
  const hide = () => (finished ? resetMove() : useMoveStore.setState({ open: false }));

  if (finished && progress) {
    const failed = progress.failed;
    const moved = progress.cancelled ? null : progress.total - failed.length;
    return (
      <Modal testId="move-dialog" labelledBy="move-title" describedBy="move-desc" onClose={hide}>
        <h2 id="move-title" className="m-0 mb-2 text-[20px] font-extrabold">
          {progress.cancelled ? t.cancelled : t.moved}
        </h2>
        <p id="move-desc" className="m-0 mb-4 text-[14.5px] text-muted tnum" role="status">
          {progress.cancelled
            ? t.cancelledBody
            : moved === progress.total
              ? t.allMoved(progress.total)
              : t.someMoved(moved ?? 0, progress.total)}
        </p>
        {failed.length > 0 && (
          <div className="mb-4">
            <p className="m-0 mb-1.5 text-[14.5px] font-semibold text-danger">{t.failed(failed.length)}</p>
            <ul className="m-0 mb-1.5 list-disc pl-5 text-[14.5px] text-text-2">
              {failed.map((f) => (
                <li key={f.infohash}>{titleOf(f.infohash)}</li>
              ))}
            </ul>
            <p className="m-0 text-[13.5px] text-muted">{t.failedHelp}</p>
          </div>
        )}
        <div className="flex justify-end">
          <button type="button" className="btn btn-line btn-sm" data-testid="move-close" onClick={hide}>
            {t.close}
          </button>
        </div>
      </Modal>
    );
  }

  const fraction = progress && progress.bytesTotal > 0 ? progress.bytesDone / progress.bytesTotal : 0;
  return (
    <Modal testId="move-dialog" labelledBy="move-title" describedBy="move-desc" onClose={hide}>
      <h2 id="move-title" className="m-0 mb-2 text-[20px] font-extrabold">
        {t.moving}
      </h2>
      <p id="move-desc" className="m-0 mb-4 text-[14.5px] text-muted tnum">
        {progress && !starting
          ? t.current(progress.index, progress.total, titleOf(progress.infohash))
          : t.preparing}
      </p>
      <div
        className="dl-bar mb-2"
        role="progressbar"
        aria-label={t.progress}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(fraction * 100)}
      >
        <i style={{ width: `${(fraction * 100).toFixed(1)}%` }} />
      </div>
      <p className="m-0 mb-6 text-[13px] text-muted tnum">
        {progress ? t.of(formatBytes(progress.bytesDone), formatBytes(progress.bytesTotal)) : " "}
        {" · "}
        {t.keepUsing}
      </p>
      <div className="flex flex-wrap justify-end gap-2.5">
        <button
          type="button"
          className="btn btn-line btn-sm"
          data-autofocus
          data-testid="move-background"
          onClick={hide}
        >
          {t.background}
        </button>
        <button
          type="button"
          className="btn btn-line btn-sm"
          disabled={cancel.isPending || cancel.isSuccess}
          data-testid="move-cancel"
          onClick={() => cancel.mutate()}
        >
          {cancel.isPending || cancel.isSuccess ? t.cancelling : t.cancel}
        </button>
      </div>
    </Modal>
  );
}
