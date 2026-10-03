import type { Torrent } from "../api/types";
import { formatBytes } from "../lib/format";
import { SOURCE_LABEL } from "../lib/versions";

/** Segmented release plate: 1080p | BluRay | x264 | 2,3 GB. HEVC renders dashed. */
export function ReleaseTag({ torrent, on = false }: { torrent: Torrent; on?: boolean }) {
  const hevc = torrent.videoCodec === "x265";
  return (
    <span className={`tag ${hevc ? "tag-hevc" : ""} ${on ? "tag-on" : ""}`}>
      <b>{torrent.quality}</b>
      <span>{SOURCE_LABEL[torrent.source]}</span>
      <span className="codec">
        {torrent.videoCodec}
        {hevc ? " · HEVC" : ""}
      </span>
      <span>{formatBytes(torrent.sizeBytes)}</span>
    </span>
  );
}
