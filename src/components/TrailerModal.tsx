import { useEffect, useRef, useState } from "react";
import { openExternalUrl, openTrailerWindow } from "../api/tauri";
import { getT, useT } from "../i18n";
import { originOf, parseTrailerMessage, TRAILER_TIMEOUT_MS, youtubeWatchUrl } from "../lib/trailer";
import { showToast } from "../store/toast";
import { Icon } from "./Icon";
import { Modal } from "./Modal";

type Props = {
  title: string;
  ytTrailerCode: string;
  /** MovieDetail.trailerUrl (the backend's local trailer page); null = skip the modal, go to the next step. */
  src: string | null;
  onClose: () => void;
};

/**
 * fallback: the modal failed to load (timeout, blocked, no API) → the separate window, then the browser.
 * browser: YouTube itself refused the video (an error code: 100 not found, 101/150 no embedding…); the
 * window shows the same page, so it goes straight to youtube.com.
 */
type Phase = "loading" | "ok" | "fallback" | "browser" | "stuck";

/**
 * Trailer: the embed in a dialog; if the player reports an error or isn't ready within 8 s, the next
 * step of the chain runs by itself (a separate window, then the browser) and the dialog closes.
 */
export function TrailerModal({ title, ytTrailerCode, src, onClose }: Props) {
  const t = useT().trailer;
  const frame = useRef<HTMLIFrameElement>(null);
  const [phase, setPhase] = useState<Phase>(src ? "loading" : "fallback");

  // Signals from the player (backend page or YouTube's own API).
  useEffect(() => {
    if (phase !== "loading" && phase !== "ok") return;
    const origin = src ? originOf(src) : null;
    const onMessage = (e: MessageEvent) => {
      if (e.origin !== origin || e.source !== frame.current?.contentWindow) return;
      const signal = parseTrailerMessage(e.data);
      if (!signal) return;
      if (signal.event === "error") setPhase(signal.code !== undefined ? "browser" : "fallback");
      else setPhase((p) => (p === "loading" ? "ok" : p));
    };
    window.addEventListener("message", onMessage);
    const timer =
      phase === "loading" ? window.setTimeout(() => setPhase("fallback"), TRAILER_TIMEOUT_MS) : undefined;
    return () => {
      window.removeEventListener("message", onMessage);
      window.clearTimeout(timer);
    };
  }, [phase, src]);

  // The rest of the chain, once: separate window → browser → (nothing worked) a message here.
  useEffect(() => {
    if (phase !== "fallback" && phase !== "browser") return;
    let gone = false;
    (async () => {
      try {
        if (phase === "browser") throw new Error("YouTube refused the embed");
        await openTrailerWindow(ytTrailerCode, title);
        if (!gone) showToast(getT().trailer.openedWindow);
      } catch {
        try {
          await openExternalUrl(youtubeWatchUrl(ytTrailerCode));
          if (!gone) showToast(getT().trailer.openedBrowser);
        } catch {
          if (!gone) setPhase("stuck");
          return;
        }
      }
      if (!gone) onClose();
    })();
    return () => {
      gone = true;
    };
  }, [phase, ytTrailerCode, title, onClose]);

  const openYoutube = () =>
    void openExternalUrl(youtubeWatchUrl(ytTrailerCode)).catch(() => showToast(t.browserFailed, "error"));

  return (
    <Modal
      labelledBy="trailer-title"
      onClose={onClose}
      testId="trailer-dialog"
      className="w-[min(1100px,92vw)] overflow-hidden rounded-lg bg-black shadow-[0_30px_80px_rgba(0,0,0,.7)]"
    >
      <div className="flex items-center gap-3 bg-surface py-2.5 pr-2.5 pl-5">
        <h2 id="trailer-title" className="m-0 min-w-0 truncate text-base font-bold">
          {t.heading(title)}
        </h2>
        <button
          type="button"
          className="ml-auto inline-flex items-center gap-1.5 rounded-md px-2 py-1.5 text-[13px] text-muted hover:text-text"
          data-testid="trailer-youtube"
          onClick={openYoutube}
        >
          <Icon name="external" size={15} />
          {t.youtube}
        </button>
        <button
          type="button"
          className="inline-grid size-10 place-items-center rounded-full text-text-2 hover:bg-white/8 hover:text-text"
          aria-label={t.close}
          data-testid="trailer-close"
          data-autofocus
          onClick={onClose}
        >
          <Icon name="x" />
        </button>
      </div>
      {src && (phase === "loading" || phase === "ok") ? (
        <iframe
          ref={frame}
          src={src}
          title={t.frame(title)}
          data-testid="trailer-frame"
          data-phase={phase}
          className="block aspect-video w-full border-0"
          allow="autoplay; encrypted-media; picture-in-picture; fullscreen"
          allowFullScreen
          // A direct YouTube embed only reports once someone listens.
          onLoad={() =>
            frame.current?.contentWindow?.postMessage(JSON.stringify({ event: "listening" }), "*")
          }
        />
      ) : (
        <div className="grid aspect-video w-full place-items-center p-6 text-center" role="status">
          {phase === "stuck" ? (
            <p className="m-0 max-w-[44ch] text-text-2">{t.stuck}</p>
          ) : (
            <p className="m-0 text-text-2">{phase === "browser" ? t.openingBrowser : t.openingWindow}</p>
          )}
        </div>
      )}
    </Modal>
  );
}
