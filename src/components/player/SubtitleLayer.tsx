import { useEffect, useState, type RefObject } from "react";
import { activeCues, cueSpans, type Cue } from "../../lib/subtitles";

type Props = {
  video: RefObject<HTMLVideoElement | null>;
  cues: readonly Cue[];
  /** Seconds; positive = later. */
  delay: number;
  /** The controls are showing: lift the text above them. */
  raised: boolean;
};

const sameCues = (a: readonly Cue[], b: readonly Cue[]) =>
  a.length === b.length && a.every((c, i) => c === b[i]);

/**
 * Draws the active cues over the video. Reads the element's currentTime on every frame (and on
 * timeupdate/seeked, for a paused video), so neither WebKitGTK's track rendering nor its event
 * timing decide what is on screen.
 */
export function SubtitleLayer({ video, cues, delay, raised }: Props) {
  const [shown, setShown] = useState<Cue[]>([]);

  useEffect(() => {
    const v = video.current;
    if (!v || cues.length === 0) {
      queueMicrotask(() => setShown([]));
      return;
    }
    let frame = 0;
    const update = () => {
      const next = activeCues(cues, v.currentTime, delay);
      setShown((prev) => (sameCues(prev, next) ? prev : next));
    };
    const loop = () => {
      update();
      frame = window.requestAnimationFrame(loop);
    };
    loop();
    v.addEventListener("timeupdate", update);
    v.addEventListener("seeked", update);
    return () => {
      window.cancelAnimationFrame(frame);
      v.removeEventListener("timeupdate", update);
      v.removeEventListener("seeked", update);
    };
  }, [video, cues, delay]);

  if (shown.length === 0) return null;
  return (
    <div
      className={`subline pointer-events-none absolute left-1/2 w-[min(80%,900px)] -translate-x-1/2 text-center ${raised ? "bottom-[150px] max-[900px]:bottom-[130px]" : "bottom-[70px]"}`}
      data-testid="subtitles"
    >
      {shown.map((cue) => (
        <p key={`${cue.start}-${cue.end}`} className="m-0 whitespace-pre-line">
          {cueSpans(cue.text).map((s, i) => (
            <span
              key={i}
              className={`${s.i ? "italic" : ""} ${s.b ? "font-extrabold" : ""} ${s.u ? "underline" : ""}`}
            >
              {s.text}
            </span>
          ))}
        </p>
      ))}
    </div>
  );
}
