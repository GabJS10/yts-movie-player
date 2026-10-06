import type { FeaturedReason, MovieDetail } from "../api/types";
import { getT } from "../i18n";
import { genreLabel } from "./genres";

/** Banner rotation: one slide every 8 s. */
export const ROTATE_MS = 8000;

/** Why this movie is in the banner. */
export function reasonLabel(reason: FeaturedReason): string {
  const t = getT().featured;
  switch (reason.kind) {
    case "because_watched":
      return t.becauseWatched(reason.sourceTitle);
    case "because_list":
      return t.becauseList(reason.sourceTitle);
    case "genre":
      return t.genre(genreLabel(reason.genre));
    case "trending":
      return t.trending;
  }
}

/** The banner's art: the sharp still first, the (blurred) background as a fallback. */
export const heroArt = (m: Pick<MovieDetail, "screenshotUrls" | "backgroundUrl">) =>
  m.screenshotUrls[0] ?? m.backgroundUrl;
