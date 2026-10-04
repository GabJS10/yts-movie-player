import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { CACHE_COMMIT_MS } from "../components/settings/StorageSection";
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
  it("has every section of the prototype; Torrent is 'Próximamente' and disabled", async () => {
    await open();
    const nav = screen.getByRole("navigation", { name: "Secciones" });
    expect(
      within(nav)
        .getAllByRole("link")
        .map((a) => a.textContent),
    ).toEqual(["Catálogo", "Subtítulos", "Reproducción", "Torrent", "Almacenamiento"]);
    expect(within(screen.getByRole("region", { name: /Subtítulos/ })).queryByText("Próximamente")).toBeNull();
    for (const name of [/Torrent/]) {
      const section = screen.getByRole("region", { name });
      expect(within(section).getByText("Próximamente")).toBeInTheDocument();
      const controls = section.querySelectorAll("input, select, [role=switch]");
      expect(controls.length).toBeGreaterThan(2);
      for (const control of controls) expect(control).toBeDisabled();
    }
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
    it("shows cache use against the limit, the library and free space", async () => {
      await open();
      const section = screen.getByRole("region", { name: "Almacenamiento" });
      expect(await within(section).findByText("3,1 GB de 10,0 GB")).toBeInTheDocument();
      expect(within(section).getByText("24,6 GB")).toBeInTheDocument();
      expect(within(section).getByText("180,0 GB")).toBeInTheDocument();
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
});
