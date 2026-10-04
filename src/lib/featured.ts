import type { FeaturedReason, MovieDetail } from "../api/types";
import { genreLabel } from "./genres";

/** Banner rotation: one slide every 8 s. */
export const ROTATE_MS = 8000;

/** Why this movie is in the banner, in Spanish. */
export function reasonLabel(reason: FeaturedReason): string {
  switch (reason.kind) {
    case "because_watched":
      return `Porque viste ${reason.sourceTitle}`;
    case "because_list":
      return `Porque tienes ${reason.sourceTitle} en Mi lista`;
    case "genre":
      return `Para ti: ${genreLabel(reason.genre)}`;
    case "trending":
      return "Tendencia en YTS";
  }
}

/** The banner's art: the sharp still first, the (blurred) background as a fallback. */
export const heroArt = (m: Pick<MovieDetail, "screenshotUrls" | "backgroundUrl">) =>
  m.screenshotUrls[0] ?? m.backgroundUrl;
