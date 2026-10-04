import { create } from "zustand";
import type { CatalogSearch } from "../lib/searchParams";

// The last search (text and filters) for the session, so "Buscar" in the navigation comes back to it
// after visiting Inicio or a movie (back/forward already keep it in the URL).

type SearchMemory = { last: CatalogSearch; remember: (search: CatalogSearch) => void };

export const useSearchMemory = create<SearchMemory>()((set) => ({
  last: {},
  remember: (last) => set({ last }),
}));

export const resetSearchMemory = () => useSearchMemory.setState({ last: {} });
