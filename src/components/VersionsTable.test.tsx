import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import type { Torrent } from "../api/types";
import { VersionsTable } from "./VersionsTable";

const base: Omit<Torrent, "infohash" | "quality" | "videoCodec" | "seeds"> = {
  source: "bluray",
  bitDepth: 8,
  audioChannels: "2.0",
  sizeBytes: 2_500_000_000,
  peers: 12,
  uploadedAt: null,
};
const torrents: Torrent[] = [
  { ...base, infohash: "a".repeat(40), quality: "720p", videoCodec: "x264", seeds: 7 },
  { ...base, infohash: "b".repeat(40), quality: "1080p", videoCodec: "x264", seeds: 120 },
  { ...base, infohash: "c".repeat(40), quality: "2160p", videoCodec: "x265", seeds: 100, bitDepth: 10 },
];

function Harness({ onSelect }: { onSelect: (h: string) => void }) {
  const [selected, setSelected] = useState<string | null>("b".repeat(40));
  return (
    <>
      <h2 id="v">Elige versión</h2>
      <VersionsTable
        torrents={torrents}
        selected={selected}
        labelledBy="v"
        onSelect={(h) => {
          setSelected(h);
          onSelect(h);
        }}
      />
    </>
  );
}

describe("VersionsTable", () => {
  it("renders one radio per version with HEVC and low-seed warnings", () => {
    render(<Harness onSelect={() => {}} />);
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(3);
    expect(radios[1]).toHaveAttribute("aria-checked", "true");
    expect(radios[0]).toHaveTextContent("Pocos seeds: arranque lento");
    expect(radios[2]).toHaveTextContent("HEVC: puede necesitar VLC");
    expect(radios[2]).toHaveTextContent("x265 · 10 bit");
    expect(radios[1]).toHaveTextContent("100+");
    expect(radios[1]).toHaveTextContent("2,3 GB");
  });

  it("selects by click and with the arrow keys", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);
    await user.click(screen.getAllByRole("radio")[0]!);
    expect(onSelect).toHaveBeenLastCalledWith("a".repeat(40));
    expect(screen.getAllByRole("radio")[0]).toHaveFocus();

    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(onSelect).toHaveBeenLastCalledWith("c".repeat(40));
    expect(screen.getAllByRole("radio")[2]).toHaveAttribute("aria-checked", "true");
    await user.keyboard("{ArrowDown}");
    expect(onSelect).toHaveBeenLastCalledWith("a".repeat(40));
  });
});
