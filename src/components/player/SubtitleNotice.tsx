import { Link } from "@tanstack/react-router";
import { useEffect, type ReactNode } from "react";
import { describeError } from "../../api/errors";
import { useT } from "../../i18n";
import { FALLBACK_LANG, formatResetTime, langName } from "../../lib/subtitles";
import { Icon } from "../Icon";
import type { SubtitleNotice as Notice } from "./useSubtitles";

const AUTO_HIDE_MS = 12_000;

type Props = {
  notice: Notice;
  onDismiss: () => void;
  onFallback: () => void;
  onPickFile: () => void;
};

const action = "font-semibold text-green hover:underline";

/** Discreet, dismissible line at the top of the player: what happened with subtitles and what to do. */
export function SubtitleNotice({ notice, onDismiss, onFallback, onPickFile }: Props) {
  useEffect(() => {
    const id = window.setTimeout(onDismiss, AUTO_HIDE_MS);
    return () => window.clearTimeout(id);
  }, [notice, onDismiss]);

  const t = useT().subtitleNotice;
  const settingsLink = (
    <Link to="/settings" hash="s-subs" className={action}>
      {t.settings}
    </Link>
  );
  const fileButton = (
    <button type="button" className={action} onClick={onPickFile}>
      {t.loadFile}
    </button>
  );

  let text: ReactNode;
  let actions: ReactNode = null;
  switch (notice.kind) {
    case "no-key":
      text = t.noKey;
      actions = settingsLink;
      break;
    case "auth":
      text = t.auth;
      actions = settingsLink;
      break;
    case "none":
      text = t.none(langName(notice.lang));
      actions =
        notice.lang !== FALLBACK_LANG ? (
          <>
            <button type="button" className={action} onClick={onFallback}>
              {t.useEnglish}
            </button>
            {fileButton}
          </>
        ) : (
          fileButton
        );
      break;
    case "quota": {
      text = t.quota(formatResetTime(notice.resetAt));
      actions = fileButton;
      break;
    }
    case "error":
      text = t.error(describeError(notice.error).title);
      actions = fileButton;
      break;
  }

  return (
    <div
      role="status"
      className="absolute top-[84px] left-1/2 z-10 flex w-max max-w-[calc(100%-32px)] -translate-x-1/2 flex-wrap items-center gap-x-4 gap-y-1.5 rounded-lg bg-black/80 py-2.5 pr-2.5 pl-4 text-sm text-text-2 shadow-[0_12px_32px_rgba(0,0,0,.5)]"
      data-testid="subtitle-notice"
    >
      <Icon name="cc" size={18} className="flex-none text-muted" />
      <span>{text}</span>
      {actions}
      <button
        type="button"
        className="inline-grid size-7 place-items-center rounded-full text-muted hover:bg-white/10 hover:text-text"
        aria-label={t.close}
        onClick={onDismiss}
      >
        <Icon name="x" size={16} />
      </button>
    </div>
  );
}
