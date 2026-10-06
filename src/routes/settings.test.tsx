import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { CACHE_COMMIT_MS } from "../components/settings/StorageSection";
import type { Download, StorageUsage } from "../api/types";
import { formatBytes } from "../lib/format";
import {
  FULL_DIR_MARK,
  MOCK_DATA_DIR,
  MOCK_PATHS,
  MOCK_PICKED_FOLDER,
  QUOTA_KEY,
  UNMOUNTED_DIR_MARK,
  type MockBackend,
} from "../mocks/backend";
import { useSubtitlesQuota } from "../store/subtitlesQuota";
import { renderApp } from "../test/render";

const GB = 1024 ** 3;
const patches = (calls: { cmd: string; args: unknown }[]) =>
  calls.filter((c) => c.cmd === "update_settings").map((c) => (c.args as { patch: unknown }).patch);

async function open(options?: Parameters<typeof renderApp>[1]) {
  const r = await renderApp("/settings", options);
  await screen.findByRole("heading", { name: "Catálogo" });
  return r;
}

describe("/settings", () => {
  it("has every section of the prototype, all of them working", async () => {
    await open();
    const nav = screen.getByRole("navigation", { name: "Secciones" });
    expect(
      within(nav)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["Idioma", "Catálogo", "Subtítulos", "Reproducción", "Torrent", "Almacenamiento", "Acerca de"]);
    expect(screen.queryByText("Próximamente")).toBeNull();
    const torrent = screen.getByRole("region", { name: /Torrent/ });
    for (const control of torrent.querySelectorAll("input, [role=switch]")) expect(control).toBeEnabled();
  });

  describe("Idioma", () => {
    it("switches the whole app to English and back, and remembers the choice", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const group = screen.getByRole("group", { name: "Idioma de la app" });
      expect(within(group).getByRole("button", { name: "Español" })).toHaveAttribute("aria-pressed", "true");

      await user.click(within(group).getByRole("button", { name: "English" }));
      expect(await screen.findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "Catalog" })).toBeInTheDocument();
      expect(screen.getByTestId("nav-downloads")).toHaveTextContent("Downloads");
      expect(document.documentElement.lang).toBe("en");
      expect(window.localStorage.getItem("yts-player:language")).toBe("en");
      // A device preference, not a backend setting.
      expect(patches(calls)).toEqual([]);

      await user.click(screen.getByRole("button", { name: "Automatic" }));
      expect(window.localStorage.getItem("yts-player:language")).toBeNull();
      await user.click(screen.getByRole("button", { name: "Español" }));
      expect(await screen.findByRole("heading", { level: 1, name: "Ajustes" })).toBeInTheDocument();
      expect(document.documentElement.lang).toBe("es");
    });
  });

  describe("Torrent", () => {
    it("saves the speed limits on Enter or blur; empty = no limit (null)", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const down = screen.getByRole("textbox", { name: "Límite de descarga" });
      expect(down).toHaveValue("");
      await user.type(down, "2048{Enter}");
      expect(patches(calls).at(-1)).toEqual({ downLimitKbps: 2048 });

      const up = screen.getByRole("textbox", { name: "Límite de subida" });
      expect(up).toHaveValue("512");
      await user.clear(up);
      await user.tab();
      expect(patches(calls).at(-1)).toEqual({ upLimitKbps: null });
      expect(
        within(screen.getByRole("region", { name: /Torrent/ })).getAllByText(/Se aplica al momento/),
      ).toHaveLength(2);
    });

    it("explains invalid numbers instead of sending them; Esc restores the saved value", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const down = screen.getByRole("textbox", { name: "Límite de descarga" });
      await user.type(down, "rápido{Enter}");
      expect(down).toHaveAttribute("aria-invalid", "true");
      expect(screen.getByText("Escribe un número entero o déjalo vacío.")).toBeInTheDocument();
      await user.keyboard("{Escape}");
      expect(down).toHaveValue("");
      expect(down).not.toHaveAttribute("aria-invalid");

      const port = screen.getByRole("textbox", { name: "Puerto de escucha" });
      await user.type(port, "80{Enter}");
      expect(screen.getByText("Entre 1024 y 65535.")).toBeInTheDocument();
      expect(patches(calls)).toEqual([]);
    });

    it("the port applies after restarting; seeding toggles at once", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const torrent = screen.getByRole("region", { name: /Torrent/ });
      expect(within(torrent).getByText(/Se aplica al reiniciar la app/)).toBeInTheDocument();
      await user.type(screen.getByRole("textbox", { name: "Puerto de escucha" }), "51413{Enter}");
      expect(patches(calls).at(-1)).toEqual({ listenPort: 51413 });

      const seed = screen.getByRole("switch", { name: "Seguir compartiendo al terminar" });
      expect(seed).toHaveAttribute("aria-checked", "false");
      await user.click(seed);
      expect(seed).toHaveAttribute("aria-checked", "true");
      expect(patches(calls).at(-1)).toEqual({ seedAfterDownload: true });
    });

    it("rolls back when the backend rejects the value", async () => {
      const user = userEvent.setup();
      await open({ fail: { update_settings: { code: "invalid_input", message: "listenPort in use" } } });
      const port = screen.getByRole("textbox", { name: "Puerto de escucha" });
      await user.type(port, "51413{Enter}");
      expect(await screen.findByRole("alert")).toHaveTextContent("No se guardó");
      await waitFor(() => expect(port).toHaveValue(""));
    });
  });

  describe("Catálogo", () => {
    it("lists the base URLs in order with their live latency", async () => {
      await open();
      const list = screen.getByRole("list", { name: "Servidores del catálogo" });
      const items = within(list).getAllByRole("listitem");
      expect(items[0]).toHaveTextContent("https://movies-api.accel.li/api/v2/");
      expect(await within(items[0]!).findByText("En uso · 182 ms")).toBeInTheDocument();
      expect(within(items[1]!).getByText("Respaldo · 240 ms")).toBeInTheDocument();
    });

    it("adds, reorders and removes servers; invalid URLs are explained, not sent", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const input = screen.getByRole("textbox", { name: "Añadir servidor" });

      await user.type(input, "otro-espejo{Enter}");
      expect(screen.getByRole("alert")).toHaveTextContent(/No es una URL válida/);
      expect(input).toHaveAttribute("aria-invalid", "true");
      expect(patches(calls)).toEqual([]);

      await user.clear(input);
      await user.type(input, "https://mirror.example/api/v2/");
      await user.click(screen.getByRole("button", { name: "Añadir" }));
      const added = [
        "https://movies-api.accel.li/api/v2/",
        "https://yts.gg/api/v2/",
        "https://mirror.example/api/v2/",
      ];
      expect(patches(calls).at(-1)).toEqual({ apiBaseUrls: added });
      expect(await screen.findByText("https://mirror.example/api/v2/")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Subir mirror.example en la lista" }));
      expect(patches(calls).at(-1)).toEqual({ apiBaseUrls: [added[0], added[2], added[1]] });

      await user.click(screen.getByRole("button", { name: "Quitar yts.gg" }));
      expect(patches(calls).at(-1)).toEqual({ apiBaseUrls: [added[0], added[2]] });
    });

    it("changing the servers refetches the catalog status without restarting", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      await screen.findByText("En uso · 182 ms");
      const before = calls.filter((c) => c.cmd === "get_api_status").length;
      await user.click(screen.getByRole("button", { name: "Quitar movies-api.accel.li" }));
      await waitFor(() =>
        expect(calls.filter((c) => c.cmd === "get_api_status").length).toBeGreaterThan(before),
      );
      const items = within(screen.getByRole("list", { name: "Servidores del catálogo" })).getAllByRole(
        "listitem",
      );
      expect(items).toHaveLength(1);
      expect(await within(items[0]!).findByText("En uso · 182 ms")).toBeInTheDocument();
      // The last one can't be removed.
      expect(screen.getByRole("button", { name: "Quitar yts.gg" })).toBeDisabled();
    });

    it("Probar conexión reports each server", async () => {
      const user = userEvent.setup();
      await open();
      await screen.findByText("En uso · 182 ms");
      await user.click(screen.getByRole("button", { name: "Probar conexión" }));
      expect(
        await screen.findByText("movies-api.accel.li responde en 182 ms · yts.gg responde en 240 ms"),
      ).toBeInTheDocument();
    });
  });

  describe("Reproducción", () => {
    it("saves quality, x264, buffer and the external player", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const quality = screen.getByRole("group", { name: "Calidad preferida" });
      expect(within(quality).getByRole("button", { name: "1080p" })).toHaveAttribute("aria-pressed", "true");
      await user.click(within(quality).getByRole("button", { name: "4K" }));
      expect(within(quality).getByRole("button", { name: "4K" })).toHaveAttribute("aria-pressed", "true");

      const x264 = screen.getByRole("switch", { name: "Preferir x264" });
      expect(x264).toHaveAttribute("aria-checked", "true");
      await user.click(x264);
      expect(x264).toHaveAttribute("aria-checked", "false");

      const buffer = screen.getByRole("group", { name: "Búfer antes de empezar" });
      expect(within(buffer).getByRole("button", { name: "8 MB" })).toHaveAttribute("aria-pressed", "true");
      await user.click(within(buffer).getByRole("button", { name: "16 MB" }));

      const player = screen.getByRole("textbox", { name: "Reproductor externo" });
      await user.clear(player);
      await user.type(player, "mpv{Enter}");

      expect(patches(calls)).toEqual([
        { preferredQuality: "2160p" },
        { preferX264: false },
        { bufferTargetBytes: 16 * 1024 * 1024 },
        { externalPlayer: "mpv" },
      ]);
    });

    it("an empty external player is not saved", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      const player = screen.getByRole("textbox", { name: "Reproductor externo" });
      await user.clear(player);
      await user.tab();
      expect(screen.getByText("Escribe el comando o la ruta del reproductor.")).toBeInTheDocument();
      expect(patches(calls)).toEqual([]);
    });

    it("when saving fails, the previous value comes back with an explanation", async () => {
      const user = userEvent.setup();
      await open({ fail: { update_settings: { code: "db", message: "readonly" } } });
      const x264 = screen.getByRole("switch", { name: "Preferir x264" });
      await user.click(x264);
      expect(await screen.findByRole("alert")).toHaveTextContent("No se pudieron guardar tus datos");
      expect(x264).toHaveAttribute("aria-checked", "true");
    });
  });

  describe("Almacenamiento", () => {
    it("shows cache use against the limit and the library", async () => {
      const { backend } = await open();
      const section = screen.getByRole("region", { name: "Almacenamiento" });
      expect(await within(section).findByText("3,1 GB de 10,0 GB")).toBeInTheDocument();
      const library = (backend.handle("get_storage_usage") as StorageUsage).libraryBytes;
      expect(within(section).getByText(formatBytes(library))).toBeInTheDocument();
      expect(within(section).getByRole("meter", { name: "Uso de la caché" })).toHaveAttribute(
        "aria-valuenow",
        "31",
      );
    });

    it("saves the limit once the slider settles, and warns when it's below the current use", async () => {
      const { calls } = await open();
      const slider = screen.getByRole("slider", { name: "Límite de la caché de streaming" });
      await screen.findByText("3,1 GB de 10,0 GB");
      fireEvent.change(slider, { target: { value: "5" } });
      fireEvent.change(slider, { target: { value: "2" } });
      expect(screen.getByText(/se borrará lo menos usado hasta bajar de 2 GB/)).toBeInTheDocument();
      expect(patches(calls)).toEqual([]);
      await act(() => new Promise((r) => setTimeout(r, CACHE_COMMIT_MS + 50)));
      expect(patches(calls)).toEqual([{ cacheLimitBytes: 2 * GB }]);
      // Usage is re-read: the mock evicts down to the new limit.
      expect(await screen.findByText("2,0 GB de 2,0 GB")).toBeInTheDocument();
    });

    it("Vaciar caché frees the space and says how much", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      await screen.findByText("3,1 GB de 10,0 GB");
      await user.click(screen.getByRole("button", { name: "Vaciar caché" }));
      expect(await screen.findByText("Caché vaciada: 3,1 GB liberados")).toBeInTheDocument();
      expect(calls.some((c) => c.cmd === "clear_cache")).toBe(true);
      expect(await screen.findByText("0 MB de 10,0 GB")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Vaciar caché" })).toBeDisabled();
    });
  });

  describe("Subtítulos", () => {
    const region = () => screen.getByRole("region", { name: "Subtítulos" });

    it("without a key, explains how to get one and can't test", async () => {
      await open({ before: (b) => b.handle("update_settings", { patch: { openSubtitlesApiKey: null } }) });
      expect(within(region()).getByText("Falta la clave de OpenSubtitles")).toBeInTheDocument();
      expect(within(region()).getByRole("button", { name: /Conseguir una clave/ })).toBeInTheDocument();
      expect(within(region()).getByRole("button", { name: "Probar" })).toBeDisabled();
    });

    it("saves the key on blur (hidden by default), and clearing it sends null", async () => {
      const user = userEvent.setup();
      const { calls } = await open({
        before: (b) => b.handle("update_settings", { patch: { openSubtitlesApiKey: null } }),
      });
      const key = within(region()).getByLabelText("Clave de API");
      expect(key).toHaveAttribute("type", "password");
      await user.click(within(region()).getByRole("button", { name: "Mostrar clave de api" }));
      expect(key).toHaveAttribute("type", "text");
      await user.type(key, "  abc123  ");
      await user.tab();
      expect(patches(calls).at(-1)).toEqual({ openSubtitlesApiKey: "abc123" });
      await waitFor(() => expect(within(region()).queryByText("Falta la clave de OpenSubtitles")).toBeNull());

      await user.clear(key);
      await user.keyboard("{Enter}");
      expect(patches(calls).at(-1)).toEqual({ openSubtitlesApiKey: null });
    });

    it("Probar reports a valid key, the session and the quota left", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      await user.click(within(region()).getByRole("button", { name: "Probar" }));
      expect(await within(region()).findByText("Clave válida")).toBeInTheDocument();

      await user.type(within(region()).getByLabelText("Usuario"), "gabriel");
      await user.type(within(region()).getByLabelText("Contraseña"), "secreto");
      // Clicking Probar blurs the password: it's saved first, then tested.
      await user.click(within(region()).getByRole("button", { name: "Probar" }));
      expect(
        await within(region()).findByText(
          /^Clave válida · sesión iniciada · quedan 20 descargas hoy \(se renueva a las \d{2}:\d{2}\)$/,
        ),
      ).toBeInTheDocument();
      expect(patches(calls)).toEqual([
        { openSubtitlesUsername: "gabriel" },
        { openSubtitlesPassword: "secreto" },
      ]);
    });

    it("Probar with no downloads left turns on the 'cupo agotado' mode", async () => {
      const user = userEvent.setup();
      await open({
        before: (b) => b.handle("update_settings", { patch: { openSubtitlesApiKey: QUOTA_KEY } }),
      });
      await user.click(within(region()).getByRole("button", { name: "Probar" }));
      await within(region()).findByText(/Clave válida/);
      expect(useSubtitlesQuota.getState().exhausted).toBe(true);
    });

    it("Probar explains a rejected key", async () => {
      const user = userEvent.setup();
      await open({
        before: (b) => b.handle("update_settings", { patch: { openSubtitlesApiKey: "invalid" } }),
      });
      await user.click(within(region()).getByRole("button", { name: "Probar" }));
      expect(await within(region()).findByText(/OpenSubtitles no acepta esta clave/)).toBeInTheDocument();
    });

    it("saves the language and automatic search", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      await user.selectOptions(within(region()).getByRole("combobox", { name: "Idioma preferido" }), "pt");
      await user.click(within(region()).getByRole("switch", { name: "Buscar subtítulos automáticamente" }));
      expect(patches(calls)).toEqual([{ subtitleLang: "pt" }, { autoSubtitles: false }]);
    });
  });

  describe("Carpetas", () => {
    const folder = (name: string) => screen.getByRole("textbox", { name });
    /** The whole path the field shows (it may draw it cut). */
    const folderPath = (name: string) => folder(name).querySelector("[data-path]")?.getAttribute("data-path");

    it("shows both folders with their disk's free space and what they hold; defaults have no Restablecer", async () => {
      await open();
      expect(folderPath("Carpeta de descargas")).toBe(`${MOCK_DATA_DIR}/library`);
      expect(folderPath("Carpeta de la caché de streaming")).toBe(`${MOCK_DATA_DIR}/cache`);
      expect(await screen.findAllByText(/^Libre en ese disco: 180,0 GB · Ocupa:/)).toHaveLength(2);
      expect(screen.queryByRole("button", { name: /^Restablecer/ })).toBeNull();
      expect(screen.queryByText(/Descargas en otra carpeta/)).toBeNull();
    });

    it("Cambiar… opens the system folder picker and applies the folder; Restablecer goes back (null)", async () => {
      const user = userEvent.setup();
      const { calls } = await open();
      await user.click(screen.getByRole("button", { name: "Cambiar la carpeta de descargas" }));
      const picker = calls.find((c) => c.cmd === "plugin:dialog|open")?.args as {
        options: { directory: boolean; defaultPath: string };
      };
      expect(picker.options).toMatchObject({ directory: true, defaultPath: `${MOCK_DATA_DIR}/library` });
      expect(patches(calls).at(-1)).toEqual({ downloadsDir: MOCK_PICKED_FOLDER });
      expect(folderPath("Carpeta de descargas")).toBe(MOCK_PICKED_FOLDER);
      expect(await screen.findByText(/^Libre en ese disco: 900,0 GB/)).toBeInTheDocument();
      // The existing downloads stayed in the old folder: offer to move them.
      expect(await screen.findByText(/5 descargas siguen en una carpeta anterior/)).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Restablecer la carpeta de descargas" }));
      expect(patches(calls).at(-1)).toEqual({ downloadsDir: null });
      await waitFor(() => expect(folderPath("Carpeta de descargas")).toBe(`${MOCK_DATA_DIR}/library`));
      await waitFor(() => expect(screen.queryByText(/Descargas en otra carpeta/)).toBeNull());
    });

    describe("on Windows", () => {
      const WIN = MOCK_PATHS.windows;

      it("shows the %LOCALAPPDATA% folders as the defaults, whole, with no Restablecer", async () => {
        await open({ platform: "windows" });
        expect(WIN.dataDir).toBe("C:\\Users\\usuario\\AppData\\Local\\yts-player");
        expect(folderPath("Carpeta de descargas")).toBe(`${WIN.dataDir}\\library`);
        expect(folder("Carpeta de descargas").querySelector("[title]")).toHaveAttribute(
          "title",
          `${WIN.dataDir}\\library`,
        );
        expect(folderPath("Carpeta de la caché de streaming")).toBe(`${WIN.dataDir}\\cache`);
        expect(await screen.findAllByText(/^Libre en ese disco: 180,0 GB/)).toHaveLength(2);
        expect(screen.queryByRole("button", { name: /^Restablecer/ })).toBeNull();
      });

      it("takes a folder on another drive, then goes back to the default", async () => {
        const user = userEvent.setup();
        const { calls } = await open({ platform: "windows" });
        await user.click(screen.getByRole("button", { name: "Cambiar la carpeta de descargas" }));
        expect(patches(calls).at(-1)).toEqual({ downloadsDir: "D:\\Películas" });
        await waitFor(() => expect(folderPath("Carpeta de descargas")).toBe("D:\\Películas"));
        expect(await screen.findByText(/^Libre en ese disco: 900,0 GB/)).toBeInTheDocument();
        expect(await screen.findByText(/5 descargas siguen en una carpeta anterior/)).toBeInTheDocument();

        await user.click(screen.getByRole("button", { name: "Restablecer la carpeta de descargas" }));
        expect(patches(calls).at(-1)).toEqual({ downloadsDir: null });
        await waitFor(() => expect(folderPath("Carpeta de descargas")).toBe(`${WIN.dataDir}\\library`));
      });

      it("treats a differently-cased default as the default (no Restablecer)", async () => {
        await open({
          platform: "windows",
          before: (b) =>
            b.handle("update_settings", {
              patch: { downloadsDir: "c:\\users\\USUARIO\\appdata\\local\\yts-player\\Library\\" },
            }),
        });
        await screen.findAllByText(/^Libre en ese disco/);
        expect(screen.queryByRole("button", { name: "Restablecer la carpeta de descargas" })).toBeNull();
      });
    });

    it("explains a folder the backend refuses and keeps the old one", async () => {
      const user = userEvent.setup();
      await open({
        fail: { update_settings: { code: "invalid_input", message: "cacheDir is not writable" } },
      });
      await user.click(screen.getByRole("button", { name: "Cambiar la carpeta de la caché de streaming" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("No se puede usar esa carpeta");
      expect(folderPath("Carpeta de la caché de streaming")).toBe(`${MOCK_DATA_DIR}/cache`);
    });

    it("warns when a folder isn't available", async () => {
      await open({
        before: (b) =>
          b.handle("update_settings", { patch: { downloadsDir: `/media/${UNMOUNTED_DIR_MARK}/pelis` } }),
      });
      expect(await screen.findByText(/aparecen como «Carpeta no disponible»/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Restablecer la carpeta de descargas" })).toBeInTheDocument();
    });

    async function startMove(dir = MOCK_PICKED_FOLDER) {
      const user = userEvent.setup();
      const r = await open({ before: (b) => b.handle("update_settings", { patch: { downloadsDir: dir } }) });
      await user.click(await screen.findByRole("button", { name: "Mover también las descargas existentes" }));
      const dialog = screen.getByRole("dialog", { name: "Moviendo descargas" });
      expect(r.calls.some((c) => c.cmd === "move_downloads")).toBe(true);
      return { ...r, user, dialog };
    }
    const tickUntil = async (backend: MockBackend, text: RegExp) => {
      for (let i = 0; i < 20 && !screen.queryByText(text); i++) {
        act(() => void backend.tick());
        await act(() => new Promise((r) => setTimeout(r, 20)));
      }
      return screen.findByText(text);
    };

    it("moves the existing downloads with progress; it can go on in the background", async () => {
      const { backend, user, dialog } = await startMove();
      expect(await within(dialog).findByText(/^Descarga 1 de 5: /)).toBeInTheDocument();
      act(() => void backend.tick());
      expect(await within(dialog).findByText(/^Descarga 2 de 5: /)).toBeInTheDocument();
      expect(
        within(dialog).getByRole("progressbar", { name: "Progreso del movimiento" }),
      ).not.toHaveAttribute("aria-valuenow", "0");

      await user.click(within(dialog).getByRole("button", { name: "Seguir en segundo plano" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      await user.click(screen.getByRole("button", { name: "Ver progreso" }));
      expect(screen.getByRole("dialog", { name: "Moviendo descargas" })).toBeInTheDocument();

      await tickUntil(backend, /Las 5 descargas ya están en la carpeta nueva/);
      await user.click(screen.getByRole("button", { name: "Cerrar" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      await waitFor(() => expect(screen.queryByText(/Descargas en otra carpeta/)).toBeNull());
      const moved = backend.handle("list_downloads") as Download[];
      expect(moved.every((d) => d.path?.startsWith(`${MOCK_PICKED_FOLDER}/`))).toBe(true);
    });

    it("Cancelar stops the move; the rest stay where they were", async () => {
      const { backend, user, dialog } = await startMove();
      await within(dialog).findByText(/^Descarga 1 de 5: /);
      act(() => void backend.tick());
      await within(dialog).findByText(/^Descarga 2 de 5: /);
      await user.click(within(dialog).getByRole("button", { name: "Cancelar" }));
      expect(await screen.findByRole("heading", { name: "Movimiento cancelado" })).toBeInTheDocument();
      expect(screen.getByText(/las demás se quedan donde estaban/)).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Cerrar" }));
      expect(await screen.findByText(/4 descargas siguen en una carpeta anterior/)).toBeInTheDocument();
    });

    it("lists the ones that couldn't move (no space)", async () => {
      const { backend } = await startMove(`/media/${FULL_DIR_MARK}`);
      await tickUntil(backend, /Se movieron 4 de 5/);
      expect(screen.getByText("No se pudo mover:")).toBeInTheDocument();
      const dialog = screen.getByRole("dialog", { name: "Descargas movidas" });
      expect(
        within(dialog)
          .getAllByRole("listitem")
          .map((li) => li.textContent),
      ).toEqual(["The Godfather (1080p)"]);
      expect(screen.getByText(/Suele ser falta de espacio/)).toBeInTheDocument();
    });
  });
});
