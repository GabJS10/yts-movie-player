import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MOCK_APP_VERSION, MOCK_PATHS, setMockUpdate } from "../../mocks/backend";
import { renderApp } from "../../test/render";

const NEW = {
  version: "1.1.0",
  url: "https://github.com/GabJS10/yts-movie-player/releases/tag/v1.1.0",
  publishedAt: "2026-11-01T10:00:00Z",
};
const openedUrls = (calls: { cmd: string; args: unknown }[]) =>
  calls.filter((c) => c.cmd === "plugin:opener|open_url").map((c) => (c.args as { url: string }).url);

async function openAbout() {
  const r = await renderApp("/settings#s-about");
  const section = await screen.findByRole("region", { name: "Acerca de" });
  return { ...r, section };
}

describe("Ajustes › Acerca de", () => {
  it("is in the section index and shows the version, up to date", async () => {
    const { section } = await openAbout();
    const nav = screen.getByRole("navigation", { name: "Secciones" });
    expect(within(nav).getByRole("link", { name: "Acerca de" })).toBeInTheDocument();
    expect(await within(section).findByText(MOCK_APP_VERSION)).toBeInTheDocument();
    expect(within(section).getByText("Tienes la última versión.")).toBeInTheDocument();
  });

  it("opens the repository, the logs folder and the credits", async () => {
    const user = userEvent.setup();
    const { section, calls } = await openAbout();
    await within(section).findByText(MOCK_APP_VERSION);
    await user.click(within(section).getByRole("button", { name: "Abrir en GitHub" }));
    await user.click(within(section).getByRole("button", { name: "Abrir carpeta de registros" }));
    await user.click(within(section).getByRole("button", { name: "OpenSubtitles" }));
    await waitFor(() =>
      expect(openedUrls(calls)).toEqual([
        "https://github.com/GabJS10/yts-movie-player",
        "https://www.opensubtitles.com",
      ]),
    );
    expect(calls.some((c) => c.cmd === "open_logs_folder")).toBe(true);
    expect(within(section).getByText(MOCK_PATHS.linux.logsDir)).toBeInTheDocument();
    for (const name of ["YTS", "OpenSubtitles", "librqbit", "Tauri"])
      expect(within(section).getByRole("button", { name })).toBeInTheDocument();
    expect(within(section).getByText(/no aloja ni distribuye películas/)).toBeInTheDocument();
  });

  it("shows the logs folder the backend reports on Windows", async () => {
    await renderApp("/settings#s-about", { platform: "windows" });
    const section = await screen.findByRole("region", { name: "Acerca de" });
    expect(
      await within(section).findByText("C:\\Users\\usuario\\AppData\\Local\\yts-player\\logs"),
    ).toBeInTheDocument();
  });

  it("explains a logs folder that can't open", async () => {
    const user = userEvent.setup();
    const { section } = await renderApp("/settings", {
      fail: { open_logs_folder: { code: "io", message: "xdg-open missing" } },
    }).then(async (r) => ({ ...r, section: await screen.findByRole("region", { name: "Acerca de" }) }));
    await user.click(within(section).getByRole("button", { name: "Abrir carpeta de registros" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("No se pudo abrir la carpeta de registros");
  });

  it("names a newer version and links to it", async () => {
    setMockUpdate(NEW);
    const user = userEvent.setup();
    const { section, calls } = await openAbout();
    expect(await within(section).findByText(/Hay una versión nueva/)).toHaveTextContent("1.1.0");
    await user.click(within(section).getByRole("button", { name: "Ver la versión 1.1.0" }));
    await waitFor(() => expect(openedUrls(calls)).toContain(NEW.url));
  });
});

describe("Aviso de nueva versión", () => {
  it("doesn't show when up to date", async () => {
    const { calls } = await renderApp("/my-list");
    await waitFor(() => expect(calls.some((c) => c.cmd === "check_for_update")).toBe(true));
    expect(screen.queryByTestId("update-notice")).toBeNull();
  });

  it("shows once, opens the release, and stays closed for the session after dismissing", async () => {
    setMockUpdate(NEW);
    const user = userEvent.setup();
    const { calls, router } = await renderApp("/my-list");
    const notice = await screen.findByTestId("update-notice");
    expect(notice).toHaveTextContent("YTS Player 1.1.0 ya está disponible.");
    await user.click(within(notice).getByRole("button", { name: "Cerrar el aviso de nueva versión" }));
    expect(screen.queryByTestId("update-notice")).toBeNull();
    // Moving around the app doesn't bring it back, nor ask GitHub again.
    await router.navigate({ to: "/downloads" });
    await screen.findByRole("heading", { level: 1, name: "Descargas" });
    expect(screen.queryByTestId("update-notice")).toBeNull();
    expect(calls.filter((c) => c.cmd === "check_for_update")).toHaveLength(1);
  });

  it("'Ver novedades' opens the release page and closes the notice", async () => {
    setMockUpdate(NEW);
    const user = userEvent.setup();
    const { calls } = await renderApp("/my-list");
    await user.click(await screen.findByRole("button", { name: "Ver novedades" }));
    await waitFor(() => expect(openedUrls(calls)).toEqual([NEW.url]));
    expect(screen.queryByTestId("update-notice")).toBeNull();
  });
});
