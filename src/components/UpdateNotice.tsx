import { useUpdateCheck } from "../api/queries";
import { openExternalUrl } from "../api/tauri";
import { useT } from "../i18n";
import { showToast } from "../store/toast";
import { useUpdateNotice } from "../store/updateNotice";
import { Icon } from "./Icon";

/**
 * Discreet "new version" card, bottom left (the toast owns the centre): the release page in the browser,
 * or close it for this session. Nothing when up to date, offline or on a failed check.
 */
export function UpdateNotice() {
  const update = useUpdateCheck().data;
  const { dismissed, dismiss } = useUpdateNotice();
  const t = useT();
  if (!update || dismissed) return null;

  const open = () => {
    dismiss();
    void openExternalUrl(update.url).catch(() => showToast(t.update.browserFailed, "error"));
  };

  return (
    <aside
      aria-label={t.update.label}
      data-testid="update-notice"
      className="toast-in fixed bottom-6 left-6 z-[150] flex max-w-[380px] items-center gap-3 rounded-lg border border-line bg-surface py-2.5 pr-2 pl-4 text-[14px] shadow-[0_12px_32px_rgba(0,0,0,.5)] max-[900px]:bottom-20"
    >
      <span className="size-2 flex-none rounded-full bg-green" aria-hidden="true" />
      <p className="m-0 text-text-2">
        <b className="text-text">YTS Player {update.version}</b>
        {t.update.available}
      </p>
      <button
        type="button"
        className="ml-1 font-semibold whitespace-nowrap text-green hover:underline"
        data-testid="update-open"
        onClick={open}
      >
        {t.update.open}
      </button>
      <button
        type="button"
        className="inline-grid size-8 flex-none place-items-center rounded-full text-muted hover:bg-white/8 hover:text-text"
        aria-label={t.update.dismiss}
        data-testid="update-dismiss"
        onClick={dismiss}
      >
        <Icon name="x" size={16} />
      </button>
    </aside>
  );
}
