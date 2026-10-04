import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { MovieDetail, Settings } from "../../api/types";
import { createMockBackend, INVALID_KEY, NO_SPANISH, QUOTA_KEY } from "../../mocks/backend";
import { renderWithProviders } from "../../test/render";
import { ExternalPlayerActions } from "./ExternalPlayerActions";

const infohashOf = (movieId: number) =>
  (createMockBackend().handle("get_movie", { movieId }) as MovieDetail).torrents[0]!.infohash;

async function openVlc(
  props: { movieId?: number; subtitles?: Parameters<typeof ExternalPlayerActions>[0]["subtitles"] } = {},
  settings: Partial<Settings> = {},
) {
  const user = userEvent.setup();
  const movieId = props.movieId ?? 1632;
  const r = await renderWithProviders(
    <ExternalPlayerActions
      movieId={movieId}
      infohash={infohashOf(movieId)}
      alternative={null}
      subtitles={props.subtitles}
    />,
    { before: (b) => b.handle("update_settings", { patch: settings }) },
  );
  await user.click(screen.getByRole("button", { name: /Abrir en VLC/ }));
  await waitFor(() => expect(r.calls.some((c) => c.cmd === "open_external_player")).toBe(true));
  const args = r.calls.find((c) => c.cmd === "open_external_player")!.args;
  return { ...r, args };
}

describe("Abrir en VLC", () => {
  it("passes the OpenSubtitles choice and the delay", async () => {
    const { args } = await openVlc({ subtitles: { subtitleId: "1632-es-2", subtitleDelayMs: -300 } });
    expect(args).toEqual({ infohash: infohashOf(1632), subtitleId: "1632-es-2", subtitleDelayMs: -300 });
  });

  it("passes the user's own file by path", async () => {
    const { args } = await openVlc({ subtitles: { subtitlePath: "/home/u/peli.srt", subtitleDelayMs: 500 } });
    expect(args).toEqual({
      infohash: infohashOf(1632),
      subtitlePath: "/home/u/peli.srt",
      subtitleDelayMs: 500,
    });
  });

  it("with no active subtitle sends nothing else (the backend picks) and, if loaded, says nothing", async () => {
    const { args } = await openVlc({ subtitles: {} });
    expect(args).toEqual({ infohash: infohashOf(1632) });
    await new Promise((r) => setTimeout(r, 30));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it.each([
    ["no key", {}, { openSubtitlesApiKey: null }, /falta la clave de OpenSubtitles/, "/settings#s-subs"],
    [
      "quota",
      {},
      { openSubtitlesApiKey: QUOTA_KEY },
      /cupo diario de OpenSubtitles \(se renueva a las \d{2}:\d{2}\)/,
      null,
    ],
    ["not found", { movieId: [...NO_SPANISH][0] }, {}, /no encontramos ninguno para esta película/, null],
    [
      "unsupported player",
      {},
      { externalPlayer: "mplayer" },
      /no admite que le pasemos subtítulos/,
      "/settings#s-play",
    ],
    [
      "subtitle error",
      {},
      { openSubtitlesApiKey: INVALID_KEY },
      /no se pudieron cargar los subtítulos/,
      null,
    ],
  ] as const)("%s: VLC opens and a discreet note explains it", async (_name, props, settings, text, link) => {
    await openVlc(props, settings);
    const note = await screen.findByRole("status");
    expect(note).toHaveTextContent(text);
    if (link) expect(screen.getByRole("link", { name: "Ir a Ajustes" })).toHaveAttribute("href", link);
    else expect(screen.queryByRole("link", { name: "Ir a Ajustes" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("subtitlesOff: forwarded as is; opens without subtitles and says nothing", async () => {
    const { args } = await openVlc({ subtitles: { subtitlesOff: true } });
    expect(args).toEqual({ infohash: infohashOf(1632), subtitlesOff: true });
    await new Promise((r) => setTimeout(r, 30));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("automatic subtitles off: opens without them, no note", async () => {
    await openVlc({}, { autoSubtitles: false });
    await new Promise((r) => setTimeout(r, 30));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
