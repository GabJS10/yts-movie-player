import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { parseTrailerMessage, TRAILER_TIMEOUT_MS, youtubeWatchUrl } from "../lib/trailer";
import { renderApp } from "../test/render";

// Interstellar's trailer in the mock catalog.
const CODE = "LY19rHKAaAg";

const frame = () => screen.getByTestId("trailer-frame") as HTMLIFrameElement;
/** A postMessage from the trailer page (same origin as trailerUrl in the mock). */
const post = (
  data: unknown,
  origin = window.location.origin,
  source: Window | null = frame().contentWindow,
) => act(() => void window.dispatchEvent(new MessageEvent("message", { data, origin, source })));

/** URLs sent to the system browser (tauri-plugin-opener). */
const openedUrls = (calls: { cmd: string; args: unknown }[]) =>
  calls.filter((c) => c.cmd === "plugin:opener|open_url").map((c) => (c.args as { url: string }).url);

async function openTrailer(options?: Parameters<typeof renderApp>[1]) {
  const user = userEvent.setup(vi.isFakeTimers() ? { advanceTimers: vi.advanceTimersByTime } : {});
  const r = await renderApp("/movie/1632", options);
  await user.click(await screen.findByTestId("movie-trailer"));
  expect(screen.getByRole("dialog", { name: "Tráiler · Interstellar" })).toBeInTheDocument();
  return { ...r, user };
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("Tráiler: modal → separate window → browser", () => {
  it("plays in the modal from MovieDetail.trailerUrl once the page says it's ready", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { calls } = await openTrailer();
    expect(frame().getAttribute("src")).toMatch(/^\/src\/mocks\/trailer\.html\?code=LY19rHKAaAg/);
    post({ source: "yts-trailer", event: "ready" });
    expect(frame()).toHaveAttribute("data-phase", "ok");
    await act(() => vi.advanceTimersByTimeAsync(TRAILER_TIMEOUT_MS * 2));
    expect(frame()).toBeInTheDocument();
    expect(calls.some((c) => c.cmd === "open_trailer_window")).toBe(false);
  });

  it("a page that can't load YouTube (error without code) opens the trailer window and closes the modal", async () => {
    const { calls } = await openTrailer();
    post({ source: "yts-trailer", event: "error" });
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "open_trailer_window")?.args).toEqual({
        ytTrailerCode: CODE,
        title: "Interstellar · Tráiler", // the window title, localized here (IPC v0.15)
      }),
    );
    expect(await screen.findByText("El tráiler se abrió en otra ventana")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("without 'ready' in 8 s it moves on too", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { calls } = await openTrailer();
    await act(() => vi.advanceTimersByTimeAsync(TRAILER_TIMEOUT_MS + 100));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_trailer_window")).toBe(true));
  });

  it("if the window can't open either, it goes to youtube.com in the browser", async () => {
    const { calls } = await openTrailer({
      fail: { open_trailer_window: { code: "internal", message: "no window" } },
    });
    post({ source: "yts-trailer", event: "error" });
    await waitFor(() => expect(openedUrls(calls)).toEqual([youtubeWatchUrl(CODE)]));
    expect(await screen.findByText("El tráiler se abrió en el navegador")).toBeInTheDocument();
  });

  it("a video YouTube refuses (ready, then error 150) goes straight to youtube.com, not to the window", async () => {
    const { calls } = await openTrailer();
    post({ source: "yts-trailer", event: "ready" });
    post({ source: "yts-trailer", event: "error", code: 150 });
    await waitFor(() => expect(openedUrls(calls)).toEqual([youtubeWatchUrl(CODE)]));
    expect(calls.some((c) => c.cmd === "open_trailer_window")).toBe(false);
    expect(await screen.findByText("El tráiler se abrió en el navegador")).toBeInTheDocument();
  });

  it("ignores messages from other origins or frames", async () => {
    await openTrailer();
    post({ source: "yts-trailer", event: "error", code: 153 }, "https://evil.example");
    post({ source: "yts-trailer", event: "error", code: 153 }, window.location.origin, window);
    expect(frame()).toHaveAttribute("data-phase", "loading");
  });

  it("'Ver en YouTube' is always there; Esc closes", async () => {
    const { user, calls } = await openTrailer();
    fireEvent.click(screen.getByTestId("trailer-youtube"));
    await waitFor(() => expect(openedUrls(calls)).toEqual([youtubeWatchUrl(CODE)]));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByTestId("movie-trailer")).toHaveFocus();
  });

  it("no trailer button without network", async () => {
    await renderApp("/movie/3304", { before: (b) => b.setOffline(true) });
    await screen.findByTestId("movie-title");
    expect(screen.queryByTestId("movie-trailer")).toBeNull();
  });
});

describe("parseTrailerMessage", () => {
  it("reads the backend page's messages and YouTube's own", () => {
    expect(parseTrailerMessage({ source: "yts-trailer", event: "error", code: 153 })).toEqual({
      event: "error",
      code: 153,
    });
    expect(parseTrailerMessage({ source: "yts-trailer", event: "ended" })).toEqual({ event: "ended" });
    expect(parseTrailerMessage({ source: "other", event: "ready" })).toBeNull();
    expect(parseTrailerMessage('{"event":"onReady"}')).toEqual({ event: "ready" });
    expect(parseTrailerMessage('{"event":"onError","info":101}')).toEqual({ event: "error", code: 101 });
    expect(parseTrailerMessage('{"event":"infoDelivery","info":{"playerState":1}}')).toEqual({
      event: "playing",
    });
    expect(parseTrailerMessage("not json")).toBeNull();
  });
});
