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

async function mount(t: Torrent, options: Parameters<typeof renderWithProviders>[1] = {}) {
  const r = await renderWithProviders(<Player movie={movie} torrent={t} />, options);
  await waitFor(() => expect(r.calls.some((c) => c.cmd === "start_stream")).toBe(true));
  await waitFor(() => expect(status()).toBe("buffering"));
  return r;
}

async function startPlaying(t: Torrent, options: Parameters<typeof renderWithProviders>[1] = {}) {
  const r = await mount(t, options);
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
    act(() => void video().play());
    fireEvent.loadedMetadata(video());
    await waitFor(() => expect(status()).toBe("codec-error"));
    expect(video().paused).toBe(true); // no audio behind the error screen (Chromium plays it)
  });

  it("offers VLC from the buffer screen when the version is not likely playable", async () => {
    await mount(x265);
    expect(screen.getByText(/probablemente no pueda mostrarla/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Abrir en VLC/ })).toBeInTheDocument();
  });

  it("offers VLC from the buffer screen for an x264 version too", async () => {
    const user = userEvent.setup();
    const { calls } = await mount(x264);
    expect(screen.queryByText(/probablemente no pueda mostrarla/)).not.toBeInTheDocument();
    expect(screen.getByText(/¿Prefieres VLC\?/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Abrir en VLC/ }));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "open_external_player")?.args).toEqual(
        expect.objectContaining({ infohash: x264.infohash }),
      ),
    );
  });

  describe("Abrir en VLC from the controls", () => {
    it("pauses the built-in video and opens VLC with the active subtitle", async () => {
      const user = userEvent.setup();
      useUiStore.setState({ subtitleDelay: { [movie.id]: 0.3 } });
      const { calls } = await startPlaying(x264);
      // The automatic Spanish subtitle is the active one.
      await waitFor(() => expect(calls.some((c) => c.cmd === "load_subtitle")).toBe(true));
      const v = video();
      act(() => void v.play());
      await user.click(screen.getByTestId("player-open-external"));
      expect(v.paused).toBe(true);
      await waitFor(() => expect(calls.some((c) => c.cmd === "open_external_player")).toBe(true));
      const args = calls.find((c) => c.cmd === "open_external_player")!.args as Record<string, unknown>;
      expect(args).toEqual({
        infohash: x264.infohash,
        subtitleId: expect.stringMatching(/^1632-es-/),
        subtitleDelayMs: 300,
      });
      useUiStore.setState({ subtitleDelay: {} });
    });

    it("V opens VLC too", async () => {
      const user = userEvent.setup();
      const { calls } = await startPlaying(x264);
      await user.keyboard("v");
      await waitFor(() => expect(calls.some((c) => c.cmd === "open_external_player")).toBe(true));
    });

    it("VLC missing: a notice explains it and links to Ajustes", async () => {
      const user = userEvent.setup();
      await startPlaying(x264, {
        fail: { open_external_player: { code: "external_player_missing", message: "vlc not found" } },
      });
      await user.click(screen.getByRole("button", { name: "Abrir en VLC (V)" }));
      const notice = await screen.findByTestId("external-notice");
      expect(notice).toHaveTextContent("No encontramos VLC");
      expect(notice).toHaveAttribute("role", "alert");
      expect(screen.getByRole("link", { name: "Ir a Ajustes" })).toHaveAttribute("href", "/settings#s-play");
      await user.click(screen.getByRole("button", { name: "Cerrar aviso" }));
      expect(screen.queryByTestId("external-notice")).not.toBeInTheDocument();
    });
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

  describe("progress", () => {
    // Mock: Spider-Verse has progress at 3720 s (1:02:00).
    const resumable = createMockBackend().handle("get_movie", { movieId: 10960 }) as MovieDetail;
    const t = resumable.torrents.find((x) => x.quality === "1080p" && x.videoCodec === "x264")!;

    async function playResumable(fromStart = false) {
      const r = await renderWithProviders(<Player movie={resumable} torrent={t} fromStart={fromStart} />);
      await waitFor(() => expect(status()).toBe("buffering"));
      push(stats(t, { phase: "ready", bufferedAheadBytes: 8 * MB }));
      await waitFor(() => expect(status()).toBe("playing"));
      const v = video();
      Object.defineProperty(v, "duration", { value: 7000, configurable: true });
      Object.defineProperty(v, "videoWidth", { value: 1920, configurable: true });
      Object.defineProperty(v, "videoHeight", { value: 800, configurable: true });
      fireEvent.loadedMetadata(v);
      return { ...r, v };
    }

    it("resumes at resumeAtS", async () => {
      const { v } = await playResumable();
      expect(v.currentTime).toBe(3720);
    });

    it("'Desde el principio' ignores the saved position", async () => {
      const { v } = await playResumable(true);
      expect(v.currentTime).toBe(0);
    });

    it("saves the position on pause and when leaving", async () => {
      const { v, calls, unmount } = await playResumable();
      act(() => void v.play());
      v.currentTime = 3800;
      act(() => v.pause());
      const saves = () => calls.filter((c) => c.cmd === "save_progress").map((c) => c.args);
      expect(saves()).toEqual([expect.objectContaining({ positionS: 3800, durationS: 7000 })]);

      act(() => void v.play());
      v.currentTime = 3900;
      fireEvent.timeUpdate(v);
      unmount();
      expect(saves().at(-1)).toEqual(expect.objectContaining({ positionS: 3900, durationS: 7000 }));
      expect((saves()[0] as { movie: { id: number } }).movie.id).toBe(10960);
    });

    it("an ended video is saved at its full duration", async () => {
      const { v, calls } = await playResumable();
      v.currentTime = 6990;
      fireEvent.ended(v);
      expect(calls.filter((c) => c.cmd === "save_progress").at(-1)?.args).toEqual(
        expect.objectContaining({ positionS: 7000, durationS: 7000 }),
      );
    });
  });

  describe("no_peers (60 s without any peer)", () => {
    const lonely = { ...x264, seeds: 0 };
    const lonelyMovie = {
      ...movie,
      torrents: movie.torrents.map((t) => (t.infohash === x264.infohash ? lonely : t)),
    };

    it("offers the best-seeded other version, waiting, or going back", async () => {
      const user = userEvent.setup();
      const r = await renderWithProviders(<Player movie={lonelyMovie} torrent={lonely} />);
      await waitFor(() => expect(r.calls.some((c) => c.cmd === "start_stream")).toBe(true));
      push(stats(lonely, { phase: "no_peers", peers: 0, downSpeedBps: 0 }));
      const panel = await screen.findByTestId("no-peers");
      expect(panel).toHaveTextContent("Nadie está compartiendo esta versión");
      // Interstellar: the x264 with most seeds besides this one (never 3D, x264 before x265).
      const best = lonelyMovie.torrents
        .filter((t) => t.infohash !== lonely.infohash && t.videoCodec === "x264" && t.quality !== "3D")
        .sort((a, b) => b.seeds - a.seeds)[0]!;
      expect(screen.getByTestId("no-peers-alternative")).toHaveAttribute(
        "href",
        `/play/1632?infohash=${best.infohash}`,
      );
      await user.click(screen.getByTestId("no-peers-wait"));
      expect(screen.queryByTestId("no-peers")).toBeNull();
      expect(screen.getByTestId("phase")).toHaveTextContent("Nadie está compartiendo esta versión");
      expect(screen.getByText("Seguimos buscando a alguien que la comparta.")).toBeInTheDocument();
    });
  });
});
