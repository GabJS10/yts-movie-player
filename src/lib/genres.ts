// Fixed YTS genre list (docs/IPC.md › Géneros). `value` goes to ListMoviesParams.genre; `label` is the UI text.
export type Genre = { value: string; label: string };

export const GENRES: readonly Genre[] = [
  { value: "action", label: "Acción" },
  { value: "adventure", label: "Aventura" },
  { value: "animation", label: "Animación" },
  { value: "biography", label: "Biografía" },
  { value: "comedy", label: "Comedia" },
  { value: "crime", label: "Crimen" },
  { value: "documentary", label: "Documental" },
  { value: "drama", label: "Drama" },
  { value: "family", label: "Familia" },
  { value: "fantasy", label: "Fantasía" },
  { value: "film-noir", label: "Cine negro" },
  { value: "history", label: "Historia" },
  { value: "horror", label: "Terror" },
  { value: "music", label: "Música" },
  { value: "musical", label: "Musical" },
  { value: "mystery", label: "Misterio" },
  { value: "romance", label: "Romance" },
  { value: "sci-fi", label: "Ciencia ficción" },
  { value: "sport", label: "Deporte" },
  { value: "thriller", label: "Suspense" },
  { value: "war", label: "Bélica" },
  { value: "western", label: "Western" },
];

const BY_VALUE = new Map(GENRES.map((g) => [g.value, g.label]));

/** Spanish label for a genre as the API spells it ("Sci-Fi") or as a filter value ("sci-fi"). */
export function genreLabel(genre: string): string {
  return BY_VALUE.get(genre.toLowerCase()) ?? genre;
}
