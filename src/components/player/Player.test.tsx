import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { MovieDetail, Torrent, TorrentStats } from "../../api/types";
import { createMockBackend } from "../../mocks/backend";
import { useSwarmStore } from "../../store/swarm";
import { useUiStore } from "../../store/ui";
import { renderWithProviders } from "../../test/render";
import { Player } from "./Player";

const MB = 1024 * 1024;
const movie = createMockBackend().handle("get_movie", { movieId: 1632 }) as MovieDetail;
const x264 = movie.torrents.find((t) => t.quality === "1080p" && t.videoCodec === "x264")!;
const x265 = movie.torrents.find((t) => t.videoCodec === "x265")!;

const stats = (t: Torrent, over: Partial<TorrentStats> = {}): TorrentStats => ({
  infohash: t.infohash,
  phase: "buffering",
  peers: 24,
  seeds: 12,
  downSpeedBps: 3 * MB,
  upSpeedBps: 0,
  progress: 0.01,
  downloadedBytes: 0,
  bufferedAheadBytes: 0,
  availableRanges: [[0, 0.01]],
  pieceMap: "1".repeat(10) + "2".repeat(30),
  pieceMapWindow: { startByte: 0, endByte: 64 * MB },
  ...over,
});
const push = (s: TorrentStats) => act(() => useSwarmStore.getState().push(s));
const status = () => document.querySelector("[data-status]")?.getAttribute("data-status");
const video = () => screen.getByTestId("video") as HTMLVideoElement;

async function mount(t: Torrent) {
  const r = await renderWithProviders(<Player movie={movie} torrent={t} />);
  await waitFor(() => expect(r.calls.some((c) => c.cmd === "start_stream")).toBe(true));
  await waitFor(() => expect(status()).toBe("buffering"));
  return r;
}

async function startPlaying(t: Torrent) {
  const r = await mount(t);
  push(stats(t, { phase: "ready", bufferedAheadBytes: 8 * MB }));
  await waitFor(() => expect(status()).toBe("playing"));
  return r;
}

afterEach(() => useSwarmStore.setState({ stats: {}, totalBps: 0 }));

describe("Player", () => {
  it("renders the buffer screen from torrent://stats and starts at bufferTargetBytes", async () => {
    await mount(x264);
    expect(screen.getByRole("heading", { name: "Interstellar" })).toBeInTheDocument();
    push(stats(x264, { phase: "buffering", bufferedAheadBytes: 4 * MB }));
    expect(await screen.findByText("Llenando el búfer…")).toBeInTheDocument();
    expect(document.querySelectorAll("[data-testid=piece-map] i.ready")).toHaveLength(10);
    expect(document.querySelectorAll("[data-testid=piece-map] i.priority")).toHaveLength(30);
    expect(screen.getByText("Próximos 64 MB desde la posición de lectura")).toBeInTheDocument();
    expect(screen.getByText("24")).toBeInTheDocument(); // peers
    expect(screen.getByText("4,0")).toBeInTheDocument(); // MB buffered of 8
    expect(screen.queryByTestId("video")).not.toBeInTheDocument();

    push(stats(x264, { bufferedAheadBytes: 8 * MB }));
    await waitFor(() => expect(status()).toBe("playing"));
    expect(video()).toHaveAttribute("src", expect.stringMatching(/^https?:/));
  });

  it("Enter forces the start", async () => {
    const user = userEvent.setup();
    await mount(x264);
    await user.keyboard("{Enter}");
    await waitFor(() => expect(status()).toBe("playing"));
  });

  it("shows 'Esperando datos…' with peers when playback runs dry", async () => {
    await startPlaying(x264);
    fireEvent.playing(video());
    push(stats(x264, { phase: "stalled", peers: 3 }));
    expect(await screen.findByText(/Esperando datos… · 3 peers/)).toBeInTheDocument();
    fireEvent.playing(video());
    await waitFor(() => expect(status()).toBe("playing"));
  });

  it("clears 'Esperando datos…' when the clock moves again, even without a playing event", async () => {
    await startPlaying(x264);
    const v = video();
    act(() => void v.play());
    fireEvent.waiting(v);
    expect(await screen.findByText(/Esperando datos…/)).toBeInTheDocument();
    v.currentTime = 50;
    fireEvent.timeUpdate(v);
    expect(status()).toBe("waiting");
    v.currentTime = 50.4;
    fireEvent.timeUpdate(v);
    await waitFor(() => expect(status()).toBe("playing"));
    expect(screen.queryByText(/Esperando datos…/)).not.toBeInTheDocument();
  });

  it("after a seek's pause, the button follows the real element (no playing event)", async () => {
    await startPlaying(x264);
    const v = video() as HTMLVideoElement & { _paused?: boolean };
    act(() => void v.play());
    expect(screen.getByRole("button", { name: "Pausar (Espacio)" })).toBeInTheDocument();

    // WebKitGTK: seeking fires `pause`, then resumes silently.
    act(() => v.pause());
    await waitFor(() => expect(status()).toBe("paused"));
    expect(screen.getByRole("button", { name: "Reproducir (Espacio)" })).toBeInTheDocument();
    fireEvent.seeking(v);
    v._paused = false; // resumed, no play/playing event
    v.currentTime = 300;
    fireEvent.timeUpdate(v);
    expect(screen.getByRole("button", { name: "Pausar (Espacio)" })).toBeInTheDocument();
    v.currentTime = 300.3;
    fireEvent.timeUpdate(v);
    await waitFor(() => expect(status()).toBe("playing"));
  });

  it("a play event without playing restores the pause icon and the status", async () => {
    await startPlaying(x264);
    const v = video() as HTMLVideoElement & { _paused?: boolean };
    act(() => void v.play());
    act(() => v.pause());
    await waitFor(() => expect(status()).toBe("paused"));
    v._paused = false;
    fireEvent.play(v);
    await waitFor(() => expect(status()).toBe("playing"));
    expect(screen.getByRole("button", { name: "Pausar (Espacio)" })).toBeInTheDocument();
  });

  it("clears it on canplay after a seek", async () => {
    await startPlaying(x264);
    const v = video();
    act(() => void v.play());
    fireEvent.waiting(v);
    await waitFor(() => expect(status()).toBe("waiting"));
    fireEvent.canPlay(v);
    await waitFor(() => expect(status()).toBe("playing"));
  });

  it("falls back to VLC and an x264 version on a decode error", async () => {
    const user = userEvent.setup();
    const { calls } = await startPlaying(x265);
    Object.defineProperty(video(), "error", { value: { code: 4 }, configurable: true });
    fireEvent.error(video());
    expect(await screen.findByText(/no puede abrir x265 \(HEVC\)/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Cambiar a 1080p x264" })).toHaveAttribute(
      "href",
      `/play/1632?infohash=${x264.infohash}`,
    );
    await user.click(screen.getByRole("button", { name: /Abrir en VLC/ }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_external_player")).toBe(true));
  });

  it("treats audio without picture (videoWidth 0) as a codec error", async () => {
    await startPlaying(x264);
    Object.defineProperty(video(), "videoWidth", { value: 0, configurable: true });
    fireEvent.loadedMetadata(video());
    await waitFor(() => expect(status()).toBe("codec-error"));
  });

  it("offers VLC from the buffer screen when the version is not likely playable", async () => {
    await mount(x265);
    expect(screen.getByText(/probablemente no pueda mostrarla/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Abrir en VLC/ })).toBeInTheDocument();
  });

  it("keyboard shortcuts drive the <video>", async () => {
    const user = userEvent.setup();
    await startPlaying(x264);
    const v = video();
    Object.defineProperty(v, "duration", { value: 600, configurable: true });
    v.currentTime = 100;
    act(() => void v.play()); // autoplay (jsdom doesn't autoplay)

    await user.keyboard(" ");
    expect(v.paused).toBe(true);
    await waitFor(() => expect(status()).toBe("paused"));
    await user.keyboard(" ");
    expect(v.paused).toBe(false);

    await user.keyboard("{ArrowRight}");
    expect(v.currentTime).toBe(110);
    await user.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(v.currentTime).toBe(90);

    const wasMuted = useUiStore.getState().muted;
    await user.keyboard("m");
    expect(useUiStore.getState().muted).toBe(!wasMuted);
    expect(v.muted).toBe(!wasMuted);
  });

  it("stops the stream when leaving", async () => {
    const { calls, unmount } = await mount(x264);
    unmount();
    expect(calls.filter((c) => c.cmd === "stop_stream").at(-1)?.args).toEqual({ infohash: x264.infohash });
  });
});
