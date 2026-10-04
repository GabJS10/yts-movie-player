import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MovieDetail, Torrent, TorrentStats } from "../../api/types";
import type { FileDropEvent } from "../../lib/fileDrop";
import { createMockBackend, type MockBackend, NO_SPANISH, QUOTA_KEY } from "../../mocks/backend";
import { useSwarmStore } from "../../store/swarm";
import { useUiStore } from "../../store/ui";
import { renderWithProviders } from "../../test/render";
import { Player } from "./Player";

// Tauri's drag & drop, driven by hand.
let dropHandler: ((e: FileDropEvent) => void) | null = null;
vi.mock("../../lib/fileDrop", () => ({
  onFileDrop: (h: (e: FileDropEvent) => void) => {
    dropHandler = h;
    return Promise.resolve(() => (dropHandler = null));
  },
}));

const MB = 1024 * 1024;
const backend = createMockBackend();
const detail = (id: number) => backend.handle("get_movie", { movieId: id }) as MovieDetail;
const x264Of = (m: MovieDetail) => m.torrents.find((t) => t.quality === "1080p" && t.videoCodec === "x264")!;

const ready = (t: Torrent): TorrentStats => ({
  infohash: t.infohash,
  phase: "ready",
  peers: 20,
  seeds: 50,
  downSpeedBps: 3 * MB,
  upSpeedBps: 0,
  progress: 0.1,
  downloadedBytes: 8 * MB,
  bufferedAheadBytes: 8 * MB,
  availableRanges: [[0, 0.1]],
  pieceMap: null,
  pieceMapWindow: null,
});

const video = () => screen.getByTestId("video") as HTMLVideoElement;
const subtitleText = () => screen.queryByTestId("subtitles")?.textContent ?? null;
/** Mock VTT: a cue every 3 s lasting 2 s ([0,2] [3,5] [6,8]…), cycling three lines per language. */
const at = (t: number) => {
  const v = video();
  v.currentTime = t;
  fireEvent.timeUpdate(v);
};

async function play(movieId: number, before?: (b: MockBackend) => void) {
  const m = detail(movieId);
  const t = x264Of(m);
  const r = await renderWithProviders(<Player movie={m} torrent={t} />, { before });
  await waitFor(() => expect(r.calls.some((c) => c.cmd === "start_stream")).toBe(true));
  await waitFor(() => expect(document.querySelector("[data-status=buffering]")).not.toBeNull());
  act(() => useSwarmStore.getState().push(ready(t)));
  await waitFor(() => expect(screen.getByTestId("video")).toBeInTheDocument());
  return { ...r, movie: m, torrent: t };
}

const cmds = (calls: { cmd: string; args: unknown }[], cmd: string) =>
  calls.filter((c) => c.cmd === cmd).map((c) => c.args);

beforeEach(() => useUiStore.setState({ subtitleDelay: {} }));
afterEach(() => useSwarmStore.setState({ stats: {}, totalBps: 0 }));

describe("Player subtitles", () => {
  it("auto-loads the first Spanish result for this release and draws the active cue", async () => {
    const { calls, torrent } = await play(1632);
    await waitFor(() => expect(cmds(calls, "load_subtitle")).toEqual([{ subtitleId: "1632-es-1" }]));
    expect(cmds(calls, "search_subtitles")[0]).toEqual({
      movieId: 1632,
      lang: "es",
      infohash: torrent.infohash,
    });
    at(0.5);
    await waitFor(() =>
      expect(subtitleText()).toBe("Nacimos aquí, pero no estábamos destinados a morir aquí."),
    );
    at(2.5);
    await waitFor(() => expect(subtitleText()).toBeNull());
    at(3.5);
    await waitFor(() => expect(subtitleText()).toBe("No entres dócilmente en esa buena noche."));
    expect(screen.getByTestId("subtitles").querySelector(".italic")).not.toBeNull(); // <i> kept
  });

  it("G/H shift the cues ±0,1 s, show it, and remember it for this movie", async () => {
    const user = userEvent.setup();
    await play(1632);
    await waitFor(() => expect(screen.getByRole("button", { name: "Subtítulos" })).toHaveClass("text-green"));
    at(3.2);
    await waitFor(() => expect(subtitleText()).not.toBeNull());
    await user.keyboard("hhhhh");
    expect(await screen.findByText("Retraso de subtítulos: +0,5 s")).toBeInTheDocument();
    at(3.2); // 2.7 in cue time: between cues
    await waitFor(() => expect(subtitleText()).toBeNull());
    await user.keyboard("g");
    expect(useUiStore.getState().subtitleDelay[1632]).toBe(0.4);
  });

  it("with nothing in Spanish, says so and offers English", async () => {
    const user = userEvent.setup();
    const [id] = [...NO_SPANISH];
    const { calls } = await play(id!);
    const notice = await screen.findByTestId("subtitle-notice");
    expect(notice).toHaveTextContent("No hay subtítulos en español para esta película.");
    expect(cmds(calls, "load_subtitle")).toEqual([]);
    await user.click(within(notice).getByRole("button", { name: "Usar inglés" }));
    await waitFor(() => expect(cmds(calls, "load_subtitle")).toEqual([{ subtitleId: `${id}-en-1` }]));
    at(0.5);
    await waitFor(() =>
      expect(subtitleText()).toBe("We were born here, but we were never meant to die here."),
    );
    expect(screen.queryByTestId("subtitle-notice")).not.toBeInTheDocument();
  });

  it("doesn't search when automatic subtitles are off", async () => {
    const { calls } = await play(1632, (b) =>
      b.handle("update_settings", { patch: { autoSubtitles: false } }),
    );
    await act(() => new Promise((r) => setTimeout(r, 50)));
    expect(cmds(calls, "search_subtitles")).toEqual([]);
    expect(screen.queryByTestId("subtitle-notice")).not.toBeInTheDocument();
  });

  it("without a key, explains it with a link to Ajustes, and a file still loads", async () => {
    const user = userEvent.setup();
    const { calls } = await play(1632, (b) =>
      b.handle("update_settings", { patch: { openSubtitlesApiKey: null } }),
    );
    const notice = await screen.findByTestId("subtitle-notice");
    expect(notice).toHaveTextContent("falta la clave de OpenSubtitles");
    expect(within(notice).getByRole("link", { name: "Ir a Ajustes › Subtítulos" })).toHaveAttribute(
      "href",
      "/settings#s-subs",
    );
    expect(cmds(calls, "search_subtitles")).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Subtítulos" }));
    const menu = screen.getByRole("menu", { name: "Subtítulos" });
    expect(within(menu).getByRole("link", { name: /Añadirla en Ajustes/ })).toBeInTheDocument();
    await user.click(within(menu).getByRole("menuitem", { name: /Cargar archivo/ }));
    await waitFor(() =>
      expect(cmds(calls, "load_subtitle_file")).toEqual([
        { path: "/home/usuario/Descargas/Interstellar.2014.es.srt" },
      ]),
    );
    expect(
      await within(menu).findByRole("menuitemradio", { name: "Interstellar.2014.es.srt" }),
    ).toHaveAttribute("aria-checked", "true");
  });

  it("when the daily quota is spent, says when it renews and offers a file", async () => {
    await play(1632, (b) => b.handle("update_settings", { patch: { openSubtitlesApiKey: QUOTA_KEY } }));
    const notice = await screen.findByTestId("subtitle-notice");
    expect(notice).toHaveTextContent(
      /Se agotó el cupo diario de OpenSubtitles; se renueva a las \d{2}:\d{2}\./,
    );
    expect(within(notice).getByRole("button", { name: "Cargar un archivo" })).toBeInTheDocument();
  });

  it("menu: release mark, choosing another option, turning them off; Escape only closes it", async () => {
    const user = userEvent.setup();
    const { calls } = await play(1632);
    await waitFor(() => expect(cmds(calls, "load_subtitle")).toHaveLength(1));
    await user.click(screen.getByRole("button", { name: "Subtítulos" }));
    const menu = screen.getByRole("menu", { name: "Subtítulos" });
    const items = await within(menu).findAllByRole("menuitemradio");
    // Desactivados + 5 options.
    expect(items).toHaveLength(6);
    expect(items[1]).toHaveAttribute("aria-checked", "true");
    expect(within(items[1]!).getByText("Tu versión")).toBeInTheDocument();
    expect(within(items[4]!).getByText(/Para sordos/)).toBeInTheDocument();
    expect(within(items[5]!).getByText(/Traducción automática/)).toBeInTheDocument();

    await user.click(items[2]!);
    await waitFor(() => expect(cmds(calls, "load_subtitle").at(-1)).toEqual({ subtitleId: "1632-es-2" }));
    await waitFor(() =>
      expect(within(menu).getAllByRole("menuitemradio")[2]).toHaveAttribute("aria-checked", "true"),
    );

    await user.click(within(menu).getByRole("menuitemradio", { name: "Desactivados" }));
    at(0.5);
    await waitFor(() => expect(subtitleText()).toBeNull());

    // English list from the language switch.
    await user.click(within(menu).getByRole("button", { name: "English" }));
    await waitFor(() =>
      expect(cmds(calls, "search_subtitles").some((a) => (a as { lang: string }).lang === "en")).toBe(true),
    );

    within(menu).getByRole("menuitemradio", { name: "Desactivados" }).focus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(cmds(calls, "stop_stream")).toEqual([]); // still in the player
    expect(screen.getByRole("button", { name: "Subtítulos" })).toHaveFocus();
  });

  it("dropping a .srt on the window loads it; other files are refused", async () => {
    const { calls } = await play(1632, (b) =>
      b.handle("update_settings", { patch: { autoSubtitles: false } }),
    );
    await waitFor(() => expect(dropHandler).not.toBeNull());
    act(() => dropHandler!({ type: "enter", paths: ["/tmp/peli.srt"] }));
    expect(screen.getByText("Suelta el archivo para cargar los subtítulos")).toBeInTheDocument();
    act(() => dropHandler!({ type: "drop", paths: ["/tmp/peli.srt"] }));
    expect(screen.queryByText("Suelta el archivo para cargar los subtítulos")).not.toBeInTheDocument();
    await waitFor(() => expect(cmds(calls, "load_subtitle_file")).toEqual([{ path: "/tmp/peli.srt" }]));

    act(() => dropHandler!({ type: "drop", paths: ["/tmp/foto.png"] }));
    expect(await screen.findByText("Solo se pueden cargar subtítulos .srt o .vtt")).toBeInTheDocument();
    expect(cmds(calls, "load_subtitle_file")).toHaveLength(1);
  });
});
