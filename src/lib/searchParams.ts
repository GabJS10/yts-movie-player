import type { ListMoviesParams, OrderBy, Quality, SortBy } from "../api/types";

/** URL search params of /search. A subset of ListMoviesParams; unknown or invalid values are dropped. */
export type CatalogSearch = Pick<
  ListMoviesParams,
  "query" | "genre" | "quality" | "minimumRating" | "sortBy" | "orderBy"
>;

const QUALITIES: readonly NonNullable<ListMoviesParams["quality"]>[] = [
  "480p",
  "720p",
  "1080p",
  "2160p",
  "3D",
  "1080p.x265",
] satisfies readonly (Quality | "1080p.x265")[];
const SORTS: readonly SortBy[] = [
  "title",
  "year",
  "rating",
  "peers",
  "seeds",
  "download_count",
  "like_count",
  "date_added",
];
const ORDERS: readonly OrderBy[] = ["desc", "asc"];

const pick = <T extends string>(allowed: readonly T[], v: unknown): T | undefined =>
  typeof v === "string" && (allowed as readonly string[]).includes(v) ? (v as T) : undefined;

export function validateCatalogSearch(raw: Record<string, unknown>): CatalogSearch {
  const out: CatalogSearch = {};
  if (typeof raw.query === "string" && raw.query.trim()) out.query = raw.query.trim();
  if (typeof raw.genre === "string" && /^[a-z-]+$/.test(raw.genre)) out.genre = raw.genre;
  const quality = pick(QUALITIES, raw.quality);
  if (quality) out.quality = quality;
  const rating = Number(raw.minimumRating);
  if (raw.minimumRating !== undefined && Number.isInteger(rating) && rating >= 0 && rating <= 9)
    out.minimumRating = rating;
  const sortBy = pick(SORTS, raw.sortBy);
  if (sortBy) out.sortBy = sortBy;
  const orderBy = pick(ORDERS, raw.orderBy);
  if (orderBy) out.orderBy = orderBy;
  return out;
}

/** /play/$movieId search params: the chosen version. Without it, the player picks the default. */
export type PlaySearch = { infohash?: string };

export function validatePlaySearch(raw: Record<string, unknown>): PlaySearch {
  const h = typeof raw.infohash === "string" ? raw.infohash.toLowerCase() : "";
  return /^[0-9a-f]{40}$/.test(h) ? { infohash: h } : {};
}
