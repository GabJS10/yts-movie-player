import { Link } from "@tanstack/react-router";
import { useEffect, useRef, type KeyboardEvent } from "react";
import { describeError } from "../../api/errors";
import { openExternalUrl } from "../../api/tauri";
import type { SubtitleOption } from "../../api/types";
import { getT, numberFormat, useT } from "../../i18n";
import { FALLBACK_LANG, formatDelay, formatResetTime, langLabel, langName } from "../../lib/subtitles";
import { useSubtitlesQuota } from "../../store/subtitlesQuota";
import { Icon } from "../Icon";
import type { Subtitles } from "./useSubtitles";

const MAX_OPTIONS = 6;

type Props = { subs: Subtitles; open: boolean; onOpenChange: (open: boolean) => void };

function OptionMeta({ o, quota }: { o: SubtitleOption; quota: boolean }) {
  const t = useT().subtitleMenu;
  const bits = [t.downloads(numberFormat().format(o.downloads))];
  if (quota && o.cached) bits.unshift(t.cached);
  if (quota && !o.cached && !o.pageUrl) bits.unshift(t.noPage);
  if (o.hearingImpaired) bits.push(t.hearingImpaired);
  if (o.aiTranslated) bits.push(t.aiTranslated);
  return (
    <span className="mt-0.5 flex flex-wrap items-center gap-x-2 text-xs font-medium text-muted">
      {o.matchesRelease && (
        <span className="rounded-xs border border-green/60 px-1 leading-4 font-bold text-green">
          {t.yourVersion}
        </span>
      )}
      {bits.join(" · ")}
    </span>
  );
}

const quotaLine = (resetAt: string | null) => getT().subtitleMenu.quota(formatResetTime(resetAt));

/** Player subtitle menu (prototype): off, options per language, "Cargar archivo…" and the delay. */
export function SubtitleMenu({ subs, open, onOpenChange }: Props) {
  const t = useT().subtitleMenu;
  const anchor = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  // Click outside closes it.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!anchor.current?.contains(e.target as Node)) onOpenChange(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open, onOpenChange]);

  // Inside the menu, Escape closes it (and doesn't leave the player); arrows move between items.
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onOpenChange(false);
      button.current?.focus();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const items = [...e.currentTarget.querySelectorAll<HTMLElement>("[role^=menuitem]:not(:disabled)")];
      const i = items.indexOf(document.activeElement as HTMLElement);
      const next = items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length];
      if (next) {
        e.preventDefault();
        e.stopPropagation();
        next.focus();
      }
    }
  };

  const { selection, options, loadingId } = subs;
  const quota = useSubtitlesQuota();
  const langs = [...new Set([subs.preferred, FALLBACK_LANG])];
  const isOn = selection.kind !== "off";

  return (
    <div className="relative" ref={anchor}>
      <button
        ref={button}
        type="button"
        className={`ctrl-btn ${isOn ? "text-green" : ""}`}
        aria-label={t.title}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
      >
        <Icon name="cc" size={26} />
      </button>
      {open && (
        <div className="menu" role="menu" aria-label={t.title} onKeyDown={onKeyDown}>
          <h3>{t.title}</h3>
          <button
            type="button"
            className="menu-item"
            role="menuitemradio"
            aria-checked={!isOn}
            onClick={subs.turnOff}
          >
            <span className="chk">{!isOn && <Icon name="check" size={18} />}</span>
            {t.off}
          </button>
          {selection.kind === "file" && (
            <button type="button" className="menu-item" role="menuitemradio" aria-checked="true">
              <span className="chk">
                <Icon name="check" size={18} />
              </span>
              <span className="min-w-0 truncate" title={selection.label}>
                {selection.label}
              </span>
            </button>
          )}

          {subs.hasKey ? (
            <>
              {langs.length > 1 && (
                <div className="flex gap-1.5 px-4 pt-2 pb-1" role="group" aria-label={t.language}>
                  {langs.map((code) => (
                    <button
                      key={code}
                      type="button"
                      aria-pressed={subs.shownLang === code}
                      onClick={() => subs.showLang(code)}
                      className="rounded-full px-3 py-1 text-[13px] font-semibold text-text-2 ring-1 ring-line-hi hover:text-text aria-pressed:bg-white/10 aria-pressed:text-text"
                    >
                      {langLabel(code)}
                    </button>
                  ))}
                </div>
              )}
              {options.isPending ? (
                <p className="m-0 px-4 py-2.5 text-sm text-muted" role="status">
                  {t.searching}
                </p>
              ) : options.isError ? (
                <p className="m-0 px-4 py-2.5 text-sm text-muted">{describeError(options.error).title}</p>
              ) : options.data.length === 0 ? (
                <p className="m-0 px-4 py-2.5 text-sm text-muted">{t.noneIn(langName(subs.shownLang))}</p>
              ) : (
                <>
                  {quota.exhausted && (
                    <p className="m-0 px-4 pt-1 pb-2 text-[13px] text-warn" data-testid="quota-line">
                      {quotaLine(quota.resetAt)}
                    </p>
                  )}
                  {options.data.slice(0, MAX_OPTIONS).map((o) => {
                    // With the quota spent, only cached files load; the rest open their page to download by hand.
                    if (quota.exhausted && !o.cached) {
                      const url = o.pageUrl;
                      return (
                        <button
                          key={o.id}
                          type="button"
                          className="menu-item disabled:cursor-not-allowed disabled:opacity-50"
                          role="menuitem"
                          disabled={!url}
                          title={url ? t.openPage : undefined}
                          onClick={() => url && void openExternalUrl(url).catch(() => undefined)}
                        >
                          <span className="chk text-text-2">{url && <Icon name="external" size={18} />}</span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate">{o.label}</span>
                            <OptionMeta o={o} quota />
                            {url && <span className="sr-only"> {t.opensPage}</span>}
                          </span>
                        </button>
                      );
                    }
                    const on = selection.kind === "option" && selection.option.id === o.id;
                    return (
                      <button
                        key={o.id}
                        type="button"
                        className="menu-item"
                        role="menuitemradio"
                        aria-checked={on}
                        disabled={loadingId === o.id}
                        onClick={() => void subs.choose(o)}
                      >
                        <span className="chk">{on && <Icon name="check" size={18} />}</span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate" title={o.label}>
                            {loadingId === o.id ? t.loading : o.label}
                          </span>
                          <OptionMeta o={o} quota={quota.exhausted} />
                        </span>
                      </button>
                    );
                  })}
                </>
              )}
            </>
          ) : (
            <div className="grid gap-1.5 px-4 py-2.5 text-sm text-muted">
              <span>{t.noKey}</span>
              <Link to="/settings" hash="s-subs" className="font-semibold text-green hover:underline">
                {t.addKey}
              </Link>
            </div>
          )}

          <button
            type="button"
            className="menu-item"
            role="menuitem"
            disabled={loadingId?.startsWith("file:")}
            onClick={() => void subs.pickFile()}
          >
            <span className="chk text-text-2">
              <Icon name="upload" size={18} />
            </span>
            {t.loadFile}
          </button>

          <hr />
          <h3>{t.sync}</h3>
          <div className="flex items-center gap-2 px-4 pt-1.5 pb-2 text-sm text-text-2">
            <span>{t.delay}</span>
            <output className="ml-auto min-w-14 text-center font-bold text-text tnum" aria-live="polite">
              {formatDelay(subs.delay)}
            </output>
            <button
              type="button"
              role="menuitem"
              className="inline-grid size-[30px] place-items-center rounded-full ring-1 ring-line-hi hover:bg-white/8"
              aria-label={t.earlier}
              onClick={() => subs.nudgeDelay(-1)}
            >
              <Icon name="minus" size={16} />
            </button>
            <button
              type="button"
              role="menuitem"
              className="inline-grid size-[30px] place-items-center rounded-full ring-1 ring-line-hi hover:bg-white/8"
              aria-label={t.later}
              onClick={() => subs.nudgeDelay(1)}
            >
              <Icon name="plus" size={16} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
