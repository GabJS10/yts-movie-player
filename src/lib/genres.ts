import { getT } from "../i18n";

// Fixed YTS genre list (docs/IPC.md › Géneros). The value goes to ListMoviesParams.genre; labels live in
// the dictionaries (src/i18n, `genres`).
export const GENRE_VALUES = [
  "action",
  "adventure",
  "animation",
  "biography",
  "comedy",
  "crime",
  "documentary",
  "drama",
  "family",
  "fantasy",
  "film-noir",
  "history",
  "horror",
  "music",
  "musical",
  "mystery",
  "romance",
  "sci-fi",
  "sport",
  "thriller",
  "war",
  "western",
] as const;

export type GenreValue = (typeof GENRE_VALUES)[number];
export type Genre = { value: GenreValue; label: string };

/** The genres with their labels in the current language. */
export const genres = (): Genre[] => {
  const labels = getT().genres;
  return GENRE_VALUES.map((value) => ({ value, label: labels[value] }));
};

const isGenre = (value: string): value is GenreValue => (GENRE_VALUES as readonly string[]).includes(value);

/** Label for a genre as the API spells it ("Sci-Fi") or as a filter value ("sci-fi"). */
export function genreLabel(genre: string): string {
  const value = genre.toLowerCase();
  return isGenre(value) ? getT().genres[value] : genre;
}
