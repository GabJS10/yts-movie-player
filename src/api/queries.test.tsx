import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { installBackend, testQueryClient } from "../test/render";
import { HERO_MIN_RATING, useHeroMovie, useInfiniteMovies, useMovie, useSuggestions } from "./queries";

function wrapper() {
  const qc = testQueryClient();
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
}

describe("query hooks (mockIPC)", () => {
  it("useInfiniteMovies pages through list_movies", async () => {
    installBackend();
    const { result } = renderHook(() => useInfiniteMovies({ genre: "animation" }), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.movies).toHaveLength(20);
    expect(result.current.data?.total).toBeGreaterThan(40);
    expect(result.current.hasNextPage).toBe(true);
    await act(() => result.current.fetchNextPage());
    await waitFor(() => expect(result.current.data?.movies).toHaveLength(40));
    expect(new Set(result.current.data?.movies.map((m) => m.id)).size).toBe(40);
  });

  it("useHeroMovie picks the most downloaded movie with artwork and a good rating", async () => {
    installBackend();
    const { result } = renderHook(() => useHeroMovie(), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.backgroundUrl).toBeTruthy();
    expect(result.current.data!.rating).toBeGreaterThanOrEqual(HERO_MIN_RATING);
  });

  it("useMovie surfaces not_found as a typed AppError", async () => {
    installBackend();
    const { result } = renderHook(() => useMovie(1), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.error).toMatchObject({ code: "not_found" });
  });

  it("useSuggestions returns other movies", async () => {
    installBackend();
    const { result } = renderHook(() => useSuggestions(1632), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data!.length).toBeGreaterThan(0);
    expect(result.current.data!.every((m) => m.id !== 1632)).toBe(true);
  });

  it("get_movie exposes sharp stills and summaries carry maxSeeds", async () => {
    installBackend();
    const { result } = renderHook(() => useMovie(1632), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    const m = result.current.data!;
    expect(m.screenshotUrls[0]).toMatch(/interstellar/);
    expect(m.maxSeeds).toBe(Math.max(...m.torrents.map((t) => t.seeds)));
  });
});
