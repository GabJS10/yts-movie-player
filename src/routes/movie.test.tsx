import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "../test/render";

describe("/movie/$movieId", () => {
  it("shows details, preselects the best x264 version and keeps actions disabled", async () => {
    await renderApp("/movie/1632");
    expect(await screen.findByRole("heading", { level: 1, name: "Interstellar" })).toBeInTheDocument();
    const radios = await screen.findAllByRole("radio");
    const checked = radios.filter((r) => r.getAttribute("aria-checked") === "true");
    expect(checked).toHaveLength(1);
    expect(checked[0]).toHaveTextContent("1080p");
    expect(screen.getByText("HEVC: puede necesitar VLC")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Reproducir 1080p/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Descargar/ })).toBeDisabled();
    expect(screen.getByText(/Matthew McConaughey/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { name: "Similares" })).toBeInTheDocument());
  });

  it("explains a missing movie and offers a way home", async () => {
    await renderApp("/movie/1");
    expect(await screen.findByText("No encontramos esta película")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Volver al inicio" })).toHaveAttribute("href", "/");
  });
});
