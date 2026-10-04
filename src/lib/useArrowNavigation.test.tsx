import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "../test/render";

// Phase 2 bug: in the real window the arrows only scrolled the page. Focus starts on <body> (and WebKit
// doesn't focus a link on click), so the row handler never ran.

const cards = () => screen.getAllByTestId("movie-card");

describe("arrow keys over the posters", () => {
  it("with nothing focused, the first arrow enters the first row on screen", async () => {
    const user = userEvent.setup();
    await renderApp("/");
    await screen.findByRole("heading", { name: "Continuar viendo" });
    (document.activeElement as HTMLElement | null)?.blur();
    expect(document.activeElement).toBe(document.body);
    await user.keyboard("{ArrowDown}");
    const focused = document.activeElement as HTMLElement;
    expect(focused).toHaveAttribute("data-card");
    // The first row on Inicio is Continuar viendo.
    expect(focused).toHaveAccessibleName(/^Continuar /);
  });

  it("starts from the card under the pointer", async () => {
    const user = userEvent.setup();
    await renderApp("/my-list");
    await waitFor(() => expect(cards().length).toBeGreaterThan(2));
    const third = cards()[2]!;
    // On screen (jsdom has no layout).
    third.getBoundingClientRect = () => ({ top: 100, bottom: 400, left: 0, right: 200 }) as DOMRect;
    fireEvent.pointerOver(third);
    (document.activeElement as HTMLElement | null)?.blur();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(third);
  });

  it("moves through a grid by line and column", async () => {
    const user = userEvent.setup();
    await renderApp("/my-list");
    await waitFor(() => expect(cards().length).toBe(5));
    // Lay the 5 cards out in 3 columns (jsdom has no layout).
    cards().forEach((c, i) => {
      Object.defineProperty(c, "offsetTop", { configurable: true, value: Math.floor(i / 3) * 300 });
      Object.defineProperty(c, "offsetLeft", { configurable: true, value: (i % 3) * 200 });
    });
    const [a, b, , , e] = cards();
    a!.focus();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(b);
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(e); // line 2, column 2
    await user.keyboard("{ArrowUp}");
    expect(document.activeElement).toBe(b);
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(a);
  });

  it("↑ from the first row goes to the banner, ↓ from the banner back to the posters", async () => {
    const user = userEvent.setup();
    await renderApp("/");
    await screen.findByRole("heading", { name: "Continuar viendo" });
    const first = (await screen.findAllByRole("link", { name: /^Continuar / }))[0]!;
    first.focus();
    await user.keyboard("{ArrowUp}");
    expect(document.activeElement).toBe(screen.getByTestId("hero-play"));
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toHaveAttribute("data-card");
  });

  it("leaves the arrows to text fields", async () => {
    const user = userEvent.setup();
    await renderApp("/search");
    const input = await screen.findByRole("searchbox", { name: "Buscar películas" });
    await waitFor(() => expect(cards().length).toBeGreaterThan(0));
    input.focus();
    await user.keyboard("{ArrowDown}");
    expect(document.activeElement).toBe(input);
  });
});
