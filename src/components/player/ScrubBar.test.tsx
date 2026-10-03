import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { scrubLayers } from "../../lib/player";
import { ScrubBar } from "./PlayerControls";

describe("ScrubBar", () => {
  it("draws watched, buffered and downloaded layers at their fractions", () => {
    const layers = scrubLayers({
      currentTime: 30,
      duration: 120,
      buffered: { length: 1, start: () => 0, end: () => 60 },
      availableRanges: [
        [0, 0.6],
        [0.8, 0.9],
      ],
    });
    render(<ScrubBar layers={layers} currentTime={30} duration={120} onSeek={vi.fn()} />);
    const rail = screen.getByTestId("scrub-rail");
    const style = (sel: string) =>
      [...rail.querySelectorAll<HTMLElement>(sel)].map((e) => [e.style.left, e.style.width]);
    expect(style("i.played")).toEqual([["", "25%"]]);
    expect(style("i.buffered")).toEqual([["0%", "50%"]]);
    expect(style("i.available")).toEqual([
      ["0%", "60%"],
      ["80%", "10%"],
    ]);
    expect(screen.getByRole("slider", { name: "Posición" })).toHaveAttribute(
      "aria-valuetext",
      "0:30 de 2:00",
    );
  });
});
