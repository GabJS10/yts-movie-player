import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FeaturedItem } from "../api/types";
import { ROTATE_MS } from "../lib/featured";
import { renderWithProviders } from "../test/render";
import { HeroBanner } from "./HeroBanner";

const hero = () => screen.getByTestId("hero");
const title = () => screen.getByTestId("hero-title").textContent;

async function renderCarousel() {
  const r = await renderWithProviders(<HeroBanner />);
  const featured = r.backend.handle("get_featured") as FeaturedItem[];
  await screen.findByRole("heading", { level: 1, name: featured[0]!.movie.title });
  return { ...r, featured };
}

function mockMatchMedia(reduced: boolean) {
  vi.stubGlobal(
    "matchMedia",
    (query: string) =>
      ({
        matches: reduced && query.includes("reduce"),
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      }) as unknown as MediaQueryList,
  );
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("HeroBanner: rotating recommendations", () => {
  it("shows each recommendation with its reason, a dot per slide and the movie's sharp still", async () => {
    const { featured } = await renderCarousel();
    // Mock: suggestions of what was watched last (Spider-Verse), of Mi lista, a genre, then trending.
    expect(featured).toHaveLength(6);
    expect(screen.getByTestId("hero-reason")).toHaveTextContent(
      "Porque viste Spider-Man: Into the Spider-Verse",
    );
    const dots = screen.getAllByTestId("hero-dot");
    expect(dots).toHaveLength(6);
    expect(dots[0]).toHaveAttribute("aria-current", "true");
    const first = featured[0]!.movie;
    expect(hero().querySelector("img.opacity-100")).toHaveAttribute(
      "src",
      first.screenshotUrls[0] ?? first.backgroundUrl,
    );
    // The next still is already requested (preload); the rest wait.
    expect(hero().querySelectorAll("img")).toHaveLength(2);
    expect(screen.getByTestId("hero-play")).toHaveAttribute(
      "href",
      expect.stringMatching(new RegExp(`^/play/${first.id}\\?infohash=`)),
    );
  });

  it("advances every 8 s, and wraps around", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { featured } = await renderCarousel();
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS - 100));
    expect(title()).toBe(featured[0]!.movie.title);
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(title()).toBe(featured[1]!.movie.title);
    expect(hero()).toHaveAttribute("data-index", "1");
    for (let i = 0; i < 5; i++) await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS));
    expect(title()).toBe(featured[0]!.movie.title);
  });

  it("stops while hovered or focused, and resumes after", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { featured } = await renderCarousel();
    fireEvent.mouseEnter(hero());
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS * 2));
    expect(title()).toBe(featured[0]!.movie.title);
    fireEvent.mouseLeave(hero());

    act(() => screen.getByTestId("hero-info").focus());
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS * 2));
    expect(title()).toBe(featured[0]!.movie.title);
    act(() => screen.getByTestId("hero-info").blur());
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS + 50));
    expect(title()).toBe(featured[1]!.movie.title);
  });

  it("stops while the window is hidden", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { featured } = await renderCarousel();
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    act(() => void document.dispatchEvent(new Event("visibilitychange")));
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS * 2));
    expect(title()).toBe(featured[0]!.movie.title);
    visibility.mockRestore();
  });

  it("with prefers-reduced-motion it never advances by itself and has no pause button", async () => {
    mockMatchMedia(true);
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { featured } = await renderCarousel();
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS * 3));
    expect(title()).toBe(featured[0]!.movie.title);
    expect(screen.getByTestId("hero-pause")).not.toBeVisible();
    // The arrows still work.
    fireEvent.click(screen.getByTestId("hero-next"));
    expect(title()).toBe(featured[1]!.movie.title);
  });

  it("the pause button stops the rotation", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { featured } = await renderCarousel();
    const pause = screen.getByRole("button", { name: "Pausar el pase automático" });
    fireEvent.click(pause);
    expect(pause).toHaveAttribute("aria-pressed", "true");
    expect(pause).toHaveAccessibleName("Reanudar el pase automático");
    fireEvent.mouseLeave(hero());
    await act(() => vi.advanceTimersByTimeAsync(ROTATE_MS * 2));
    expect(title()).toBe(featured[0]!.movie.title);
  });

  it("moves with the arrows, the dots and ←/→, and announces the slide", async () => {
    const user = userEvent.setup();
    const { featured } = await renderCarousel();
    await user.click(screen.getByRole("button", { name: "Siguiente destacada" }));
    expect(title()).toBe(featured[1]!.movie.title);
    await user.click(screen.getByRole("button", { name: "Anterior destacada" }));
    await user.click(screen.getByRole("button", { name: "Anterior destacada" }));
    expect(title()).toBe(featured[5]!.movie.title);
    await user.click(screen.getAllByTestId("hero-dot")[3]!);
    expect(title()).toBe(featured[3]!.movie.title);
    expect(screen.getAllByTestId("hero-dot")[3]).toHaveAttribute("aria-current", "true");

    await user.keyboard("{ArrowRight}"); // focus is on the dot, inside the banner
    expect(title()).toBe(featured[4]!.movie.title);
    await user.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(title()).toBe(featured[2]!.movie.title);
    expect(within(hero()).getByText(`Destacada 3 de 6: ${featured[2]!.movie.title}`)).toHaveAttribute(
      "aria-live",
      "polite",
    );
  });

  it("♥ adds the current recommendation to Mi lista", async () => {
    const user = userEvent.setup();
    const { calls, featured } = await renderCarousel();
    await user.click(screen.getByRole("button", { name: "Añadir a Mi lista" }));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "add_favorite")?.args).toMatchObject({
        movie: { id: featured[0]!.movie.id },
      }),
    );
  });
});

describe("HeroBanner: without recommendations", () => {
  it("falls back to the most downloaded movie, with its sharp still and default version", async () => {
    const { container, calls } = await renderWithProviders(<HeroBanner />, {
      fail: { get_featured: { code: "internal", message: "boom" } },
    });
    expect(
      await screen.findByRole("heading", { level: 1, name: "Avengers: Infinity War" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(container.querySelector("section > img")).toHaveAttribute(
        "src",
        "/design/prototype/bg/infinity-war.jpg",
      ),
    );
    // get_featured is retried once before falling back.
    expect([...new Set(calls.map((c) => c.cmd))].sort()).toEqual([
      "get_featured",
      "get_movie",
      "get_settings",
      "list_movies",
    ]);
    expect(await screen.findByText("1080p")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Más info/ })).toHaveAttribute("href", "/movie/8462");
    expect(screen.queryByTestId("hero-dot")).toBeNull();
    expect(screen.queryByTestId("hero-reason")).toBeNull();
  });

  it("offline (empty list) also shows the single banner", async () => {
    await renderWithProviders(<HeroBanner />, {
      before: (b) => {
        // An empty answer without failing, as the backend does without network.
        const handle = b.handle;
        b.handle = (cmd, args) => (cmd === "get_featured" ? [] : handle(cmd, args));
      },
    });
    expect(
      await screen.findByRole("heading", { level: 1, name: "Avengers: Infinity War" }),
    ).toBeInTheDocument();
  });
});
