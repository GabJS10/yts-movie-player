import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "../test/render";

const searchOf = (router: Awaited<ReturnType<typeof renderApp>>["router"]) =>
  router.state.matches.find((m) => m.routeId === "/search")?.search;

describe("/search filters ↔ URL", () => {
  it("reads filters from the URL and sends them to list_movies", async () => {
    const { calls } = await renderApp("/search?genre=horror&quality=1080p&minimumRating=6");
    expect(screen.getByLabelText("Género")).toHaveValue("horror");
    const quality = screen.getByRole("group", { name: "Calidad" });
    expect(within(quality).getByRole("button", { name: "1080p" })).toHaveAttribute("aria-pressed", "true");
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_movies")).toBe(true));
    const { params } = calls.find((c) => c.cmd === "list_movies")!.args as {
      params: Record<string, unknown>;
    };
    expect(params).toMatchObject({ genre: "horror", quality: "1080p", minimumRating: 6, page: 1 });
  });

  it("writes filter changes to the URL, and back undoes them", async () => {
    const user = userEvent.setup();
    const { router } = await renderApp("/search");
    await user.selectOptions(screen.getByLabelText("Género"), "comedy");
    await waitFor(() => expect(searchOf(router)).toMatchObject({ genre: "comedy" }));
    await user.click(
      within(screen.getByRole("group", { name: "Valoración mínima" })).getByRole("button", { name: "7+" }),
    );
    await waitFor(() => expect(searchOf(router)).toMatchObject({ genre: "comedy", minimumRating: 7 }));

    act(() => router.history.back());
    await waitFor(() => expect(searchOf(router)).toEqual({ genre: "comedy" }));
    expect(
      within(screen.getByRole("group", { name: "Valoración mínima" })).getByRole("button", { name: "Todas" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("debounces typing into the query param", async () => {
    const user = userEvent.setup();
    const { router, calls } = await renderApp("/search");
    await user.type(screen.getByLabelText("Buscar películas"), "interstellar");
    expect(searchOf(router)?.query).toBeUndefined();
    await waitFor(() => expect(searchOf(router)).toMatchObject({ query: "interstellar" }));
    expect(await screen.findByText("1 película")).toBeInTheDocument();
    const queries = calls
      .filter((c) => c.cmd === "list_movies")
      .map((c) => (c.args as { params: { query?: string } }).params.query);
    // No request per keystroke: only the initial (empty) query and the final one.
    expect(queries).toEqual([undefined, "interstellar"]);
  });

  it("shows its own empty state and clears filters", async () => {
    const user = userEvent.setup();
    const { router } = await renderApp("/search?query=zzzz-no-such-title&genre=horror");
    expect(await screen.findByText("Nada coincide con «zzzz-no-such-title»")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Quitar filtros" })[0]!);
    await waitFor(() => expect(searchOf(router)).toEqual({}));
    expect(screen.getByLabelText("Buscar películas")).toHaveValue("");
  });

  it("keeps the search and its filters when leaving through the navigation and coming back", async () => {
    const user = userEvent.setup();
    const { router } = await renderApp("/search?query=the&genre=action&quality=1080p");
    await screen.findByRole("searchbox", { name: "Buscar películas" });
    const nav = screen.getByRole("navigation", { name: "Principal" });
    await user.click(within(nav).getByRole("link", { name: "Inicio" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/"));

    await user.click(within(nav).getByRole("link", { name: "Buscar" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/search"));
    expect(router.state.location.search).toMatchObject({ query: "the", genre: "action", quality: "1080p" });
    expect(screen.getByRole("searchbox", { name: "Buscar películas" })).toHaveValue("the");
    expect(screen.getByTestId("filter-genre")).toHaveValue("action");
    expect(
      within(screen.getByTestId("filter-quality")).getByRole("button", { name: "1080p" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(within(nav).getByRole("link", { name: "Buscar" })).toHaveAttribute("aria-current", "page");
  });
});
