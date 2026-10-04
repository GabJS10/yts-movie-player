// Fake YTS API for the real-app E2E tests (no internet needed).
//
// Serves the real fixtures from src-tauri/tests/fixtures, rewritten so that:
// - every image points back to this server (a tiny PNG),
// - the first movie of list_movies has a single 720p x264 torrent whose hash and
//   .torrent file come from the local seeder (e2e_seeder), so it can really play.
//
// Usage: startFakeYts({ port, seeder: { infohash, torrentPath, sizeBytes } })
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixturesDir = path.resolve(here, "../../src-tauri/tests/fixtures");
const load = (name) => JSON.parse(readFileSync(path.join(fixturesDir, name), "utf8"));

// 1x1 grey PNG.
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);

const IMAGE_KEYS = [
  "background_image",
  "background_image_original",
  "small_cover_image",
  "medium_cover_image",
  "large_cover_image",
  "medium_screenshot_image1",
  "medium_screenshot_image2",
  "medium_screenshot_image3",
  "large_screenshot_image1",
  "large_screenshot_image2",
  "large_screenshot_image3",
];

export function startFakeYts({ port = 0, seeder }) {
  const list = load("list_movies.json");
  const details = load("movie_details.json");
  const suggestions = load("movie_suggestions.json");
  const playableId = list.data.movies[0].id;

  /** @param {string} base */
  const rewriteMovie = (movie, base) => {
    const m = structuredClone(movie);
    for (const key of IMAGE_KEYS) {
      if (typeof m[key] === "string") m[key] = `${base}/img/${m.id}/${key}.png`;
    }
    for (const c of m.cast ?? []) {
      if (c.url_small_image) c.url_small_image = `${base}/img/cast/${encodeURIComponent(c.name)}.png`;
    }
    if (m.id === playableId && seeder) {
      m.torrents = [
        {
          url: `${base}/torrent/download/${seeder.infohash.toUpperCase()}`,
          hash: seeder.infohash.toUpperCase(),
          quality: "720p",
          type: "web",
          is_repack: "0",
          video_codec: "x264",
          bit_depth: "8",
          audio_channels: "2.0",
          seeds: 1,
          peers: 0,
          size: "1 MB",
          size_bytes: seeder.sizeBytes,
          date_uploaded: "2026-10-04 00:00:00",
          date_uploaded_unix: 1791072000,
        },
      ];
    }
    return m;
  };

  const ok = (data) => ({ status: "ok", status_message: "Query was successful", data });

  const server = createServer((req, res) => {
    const base = `http://127.0.0.1:${server.address().port}`;
    const url = new URL(req.url ?? "/", base);
    const json = (body, status = 200) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(body));
    };
    const p = url.pathname.replace(/^\/api\/v2/, "");

    if (p === "/list_movies.json") {
      const query = (url.searchParams.get("query_term") ?? "").toLowerCase();
      const page = Number(url.searchParams.get("page") ?? "1");
      let movies = list.data.movies.map((m) => rewriteMovie(m, base));
      if (query) movies = movies.filter((m) => m.title.toLowerCase().includes(query));
      if (page > 1) movies = [];
      const data = { movie_count: movies.length, limit: 20, page_number: page };
      return json(ok(movies.length ? { ...data, movies } : data));
    }
    if (p === "/movie_details.json") {
      const id = Number(url.searchParams.get("movie_id"));
      const fromList = list.data.movies.find((m) => m.id === id);
      if (!fromList) return json(ok({ movie: { id: 0 } }));
      // Real details shape, with the identity and torrents of the listed movie.
      const movie = { ...details.data.movie, ...fromList, cast: details.data.movie.cast };
      return json(ok({ movie: rewriteMovie(movie, base) }));
    }
    if (p === "/movie_suggestions.json") {
      const movies = suggestions.data.movies.map((m) => rewriteMovie(m, base));
      return json(ok({ movie_count: 0, movies }));
    }
    if (p.startsWith("/img/")) {
      res.writeHead(200, { "content-type": "image/png" });
      return res.end(PNG);
    }
    if (p.startsWith("/torrent/download/") && seeder) {
      res.writeHead(200, { "content-type": "application/x-bittorrent" });
      return res.end(readFileSync(seeder.torrentPath));
    }
    json({ status: "error", status_message: `unknown endpoint ${p}` }, 404);
  });

  return new Promise((resolve) => {
    server.listen(port, "127.0.0.1", () => {
      const base = `http://127.0.0.1:${server.address().port}`;
      resolve({ server, baseUrl: `${base}/api/v2/`, playableId, close: () => server.close() });
    });
  });
}

// Standalone: node app/fake-yts.mjs [port] → serves without a seeder (catalog only).
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { baseUrl } = await startFakeYts({ port: Number(process.argv[2] ?? 0) });
  console.log(`fake YTS at ${baseUrl}`);
}
