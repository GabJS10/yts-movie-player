import type { ListMoviesParams } from "../api/types";

export type RowDef = { title: string; params: Omit<ListMoviesParams, "page">; note?: string };

/**
 * Catalog rows with the genre ones by affinity (get_home_profile.genreOrder); genres it doesn't rank keep
 * the default order, after the ranked ones. The general rows (trending, new, top rated) always come first.
 */
export function orderRows(rows: readonly RowDef[], genreOrder: readonly string[]): RowDef[] {
  const rank = (r: RowDef) => {
    const i = r.params.genre ? genreOrder.indexOf(r.params.genre) : -1;
    return i < 0 ? genreOrder.length : i;
  };
  const general = rows.filter((r) => !r.params.genre);
  const genres = rows
    .map((r, i) => ({ r, i }))
    .filter(({ r }) => r.params.genre)
    .sort((a, b) => rank(a.r) - rank(b.r) || a.i - b.i)
    .map(({ r }) => r);
  return [...general, ...genres];
}
