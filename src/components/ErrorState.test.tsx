import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ERROR_NEXT } from "../api/errors";
import { es } from "../i18n/es";
import type { ErrorCode } from "../api/types";
import { renderApp, renderWithProviders } from "../test/render";
import { ErrorState } from "./ErrorState";

const ERROR_COPY = es.errors;
const CODES = Object.keys(ERROR_COPY) as ErrorCode[];

describe("ErrorState: a screen with a way out for every ErrorCode", () => {
  it.each(CODES)("%s: Spanish explanation, and retry and/or a place to fix it", async (code) => {
    const onRetry = vi.fn();
    await renderWithProviders(<ErrorState error={{ code, message: "technical detail" }} onRetry={onRetry} />);
    const alert = await screen.findByTestId("error-state");
    expect(alert).toHaveAttribute("data-code", code);
    expect(alert).toHaveTextContent(ERROR_COPY[code].title);
    expect(alert).toHaveTextContent(ERROR_COPY[code].action);
    expect(alert).not.toHaveTextContent("technical detail");
    const next = ERROR_NEXT[code];
    expect(screen.queryByTestId("error-retry") !== null).toBe(next.retry);
    const link = screen.queryByTestId("error-link");
    if (next.link)
      expect(link).toHaveAttribute("href", `${next.link.to}${next.link.hash ? `#${next.link.hash}` : ""}`);
    else expect(link).toBeNull();
    // Never a dead end: there's always something to click.
    expect(alert.querySelectorAll("button, a").length).toBeGreaterThan(0);
  });

  it("offers the place's way back when given", async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();
    await renderWithProviders(
      <ErrorState error={{ code: "torrent", message: "x" }} onBack={onBack} backLabel="Volver a la ficha" />,
    );
    await user.click(await screen.findByRole("button", { name: "Volver a la ficha" }));
    expect(onBack).toHaveBeenCalled();
  });

  it("in the app: a disk error on the movie page leads to Ajustes › Almacenamiento", async () => {
    await renderApp("/movie/1632", { fail: { get_movie: { code: "io", message: "EACCES" } } });
    expect(await screen.findByRole("link", { name: "Ir a Ajustes › Almacenamiento" })).toHaveAttribute(
      "href",
      "/settings#s-disk",
    );
  });

  it("in the player: no_peers from start_stream can retry or go back to the movie", async () => {
    await renderApp("/play/1632", {
      fail: { start_stream: { code: "no_peers", message: "60 s, no peers" } },
    });
    expect(await screen.findByText("Nadie está compartiendo esta versión")).toBeInTheDocument();
    expect(screen.getByTestId("error-retry")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Volver a la ficha" })).toBeInTheDocument();
  });
});
