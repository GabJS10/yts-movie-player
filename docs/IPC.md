# Contrato IPC (frontend ⇄ backend)

**Versión:** v0.8 (borrador para el MVP), **Dueño:** `plan`. `backend` propone los cambios y `frontend` los implementa en `src/api/tauri.ts`. Un cambio que rompa el contrato se coordina antes con `plan` (ver `AGENTS.md`).

Este documento es la única fuente de verdad sobre los comandos Tauri, los eventos y los tipos compartidos. Si el código y este archivo no coinciden, el bug está en el código o el archivo está desactualizado: hay que corregir uno de los dos en el mismo cambio.

## Convenciones

- **Comandos:** `snake_case` en Rust (`#[tauri::command]`). Desde el front se llaman con ese mismo nombre: `invoke("list_movies", { params })`.
- **Argumentos:** se pasan en `camelCase` (Tauri convierte `movie_id` en `movieId`). Cuando hay más de 2 parámetros se agrupan en un solo objeto.
- **Payloads:** usan `#[serde(rename_all = "camelCase")]`. Los enums van como strings en `snake_case`.
- **Unidades:** bytes como `number` (entero), velocidades en **bytes/s**, tiempos de reproducción en **segundos** (`number`, con decimales) y fechas en **ISO 8601 UTC** (`string`).
- **`infohash`:** hex de 40 caracteres en **minúsculas**. Identifica cada torrent, sea de streaming o de descarga.
- **URLs que recibe el front:** siempre listas para usar en `<img>`, `<video>` o `<track>`. Las imágenes, los streams y los subtítulos se sirven desde el servidor local `http://127.0.0.1:<port>/…`, así que **el front nunca construye URLs ni habla con dominios de YTS**.
- **Campos opcionales en las salidas** (lo que devuelve el backend y los payloads de eventos): `T | null`, nunca `undefined`. En Rust son `Option<T>` sin `skip_serializing_if`.
- **URLs del servidor local = solo para la sesión actual:** llevan el puerto aleatorio del arranque. El front no las guarda en ningún sitio persistente. Lo que el backend guarda en SQLite (favoritos, progreso, descargas) se guarda sin el origen, y al leerlo se le pone el puerto actual (el path `/img/<sha256-de-la-url-remota>` es estable).
- **Campos opcionales en las entradas** (argumentos de los comandos): `campo?: T`, que se pueden omitir. En Rust son `Option<T>` con `#[serde(default)]`. Una clave omitida significa "usar el valor por defecto". La única excepción es `SettingsPatch` (ver más abajo).

## Errores

Todo comando devuelve `Result<T, AppError>`. Si hay error, `invoke` rechaza la promesa con este objeto:

```ts
type AppError = {
  code: ErrorCode;
  message: string;  // detalle técnico (en inglés, para logs). La UI NO lo muestra tal cual
};

type ErrorCode =
  | "network"                  // sin conexión o timeout
  | "api_unavailable"          // fallaron todas las URLs base de YTS
  | "not_found"                // la película, el torrent o el recurso no existe
  | "invalid_input"
  | "torrent"                  // error del motor torrent
  | "no_peers"                 // no hubo peers tras el timeout
  | "subtitles_auth"           // falta la API key de OpenSubtitles o es inválida
  | "subtitles_quota"          // se agotó la cuota diaria de OpenSubtitles
  | "external_player_missing"  // VLC no está instalado
  | "io"                       // disco lleno, permisos, etc.
  | "db"
  | "internal";
```

El frontend convierte `code` en un texto en español y en una acción siguiente (principio de producto: "nunca dejar al usuario sin salida").

## Tipos compartidos

```ts
type Quality = "480p" | "720p" | "1080p" | "2160p" | "3D";
type VideoCodec = "x264" | "x265";

type Torrent = {
  infohash: string;
  quality: Quality;
  source: "bluray" | "web";        // `type` en la API de YTS; un valor desconocido se convierte en "web" (con un warn en el log)
  videoCodec: VideoCodec;
  bitDepth: number | null;         // 8 | 10
  audioChannels: string | null;    // "2.0", "5.1"
  sizeBytes: number;
  seeds: number;
  peers: number;
  uploadedAt: string | null;
};

type MovieSummary = {
  id: number;                      // id de YTS
  imdbCode: string;
  title: string;
  year: number;
  rating: number;                  // IMDb 0–10
  runtimeMin: number;
  genres: string[];                // con las mayúsculas de la API ("Sci-Fi"); para filtrar se usa el valor en minúsculas (ver Géneros)
  coverUrl: string | null;         // portada mediana, servida desde caché local; null si YTS no trae portada (el front muestra un placeholder)
  coverLargeUrl: string | null;    // si falta la grande se usa la mediana
  backgroundUrl: string | null;
  qualities: Quality[];            // calidades disponibles, sin duplicados
  hasX264: boolean;
  maxSeeds: number;                // el máximo de seeds entre sus torrents (señal del enjambre en las tarjetas); 0 si no hay torrents
};

type CastMember = { name: string; character: string | null; imageUrl: string | null };

type MovieDetail = MovieSummary & {
  summary: string;
  language: string;
  mpaRating: string | null;
  ytTrailerCode: string | null;
  screenshotUrls: string[];        // capturas grandes nítidas (1280 px, large_screenshot_image1..3); [] si no hay. El hero y la ficha usan [0] ?? backgroundUrl (background_image viene pequeño y desenfocado)
  cast: CastMember[];
  torrents: Torrent[];             // en el orden de la API; el front decide cómo mostrarlos
  isFavorite: boolean;
  progress: Progress | null;
  download: Download | null;       // si ya hay una descarga de cualquier versión
};

type MoviePage = {
  movies: MovieSummary[];          // [] si no hay resultados, nunca null
  total: number;                   // movie_count
  page: number;
  limit: number;
  hasMore: boolean;
};

type Progress = {
  movieId: number;
  positionS: number;
  durationS: number;
  finished: boolean;               // true a partir del 92 %
  updatedAt: string;
};
```

## Comandos

### Catálogo

| Comando | Argumentos | Devuelve |
|---|---|---|
| `list_movies` | `{ params: ListMoviesParams }` | `MoviePage` |
| `get_movie` | `{ movieId: number }` | `MovieDetail` |
| `get_suggestions` | `{ movieId: number }` | `MovieSummary[]` |
| `get_api_status` | — | `ApiEndpointStatus[]` |

```ts
type ListMoviesParams = {
  page?: number;                   // por defecto 1
  limit?: number;                  // 1–50, por defecto 20
  query?: string;                  // query_term
  genre?: string;                  // en minúsculas como en la API: "action", "sci-fi"…
  quality?: Quality | "1080p.x265";
  minimumRating?: number;          // 0–9
  sortBy?: "title" | "year" | "rating" | "peers" | "seeds" | "download_count" | "like_count" | "date_added";
  orderBy?: "desc" | "asc";
};

type ApiEndpointStatus = {
  baseUrl: string;
  role: "active" | "fallback";
  latencyMs: number | null;        // null si falló
  ok: boolean;
};
```

**Géneros:** no hay comando para listarlos (la API de YTS no los expone). El front tiene una lista fija en `src/lib/genres.ts` de pares `{ value, label }`: `value` es lo que se manda en `ListMoviesParams.genre` (minúsculas, p. ej. `"sci-fi"`) y `label` es el texto en español. Géneros de YTS: action, adventure, animation, biography, comedy, crime, documentary, drama, family, fantasy, film-noir, history, horror, music, musical, mystery, romance, sci-fi, sport, thriller, war, western.

El caché (TTL de unos 30 minutos) y el failover entre URLs base son internos del backend y no cambian el contrato. `get_api_status` alimenta la sección "Catálogo" de Ajustes.

### Reproducción (streaming)

| Comando | Argumentos | Devuelve |
|---|---|---|
| `start_stream` | `{ movieId: number, infohash: string }` | `StreamSession` |
| `stop_stream` | `{ infohash: string }` | `void` |
| `open_external_player` | `{ infohash: string, subtitleId?: string, subtitlePath?: string, subtitleDelayMs?: number }` | `ExternalPlayerResult` |

```ts
type StreamSession = {
  infohash: string;
  movieId: number;
  streamUrl: string;               // http://127.0.0.1:<port>/stream/<infohash>/<fileIdx> (soporta Range → 206)
  fileName: string;
  fileSizeBytes: number;
  videoCodec: VideoCodec;
  likelyPlayable: boolean;         // false para x265: el front ofrece VLC desde el principio
  bufferTargetBytes: number;       // umbral para empezar (≈ 8 MB, configurable)
  resumeAtS: number | null;        // progreso guardado, si existe
  source: "network" | "library";   // "library" si la película ya está descargada; arranque inmediato
};
```

- `start_stream` obtiene el torrent bajando el archivo `.torrent` de YTS (`torrents[].url`, p. ej. `https://yts.gg/torrent/download/<HASH>`, que ya trae los trackers): así se salta la espera de metadatos, que es lo más lento al arrancar. Si la descarga falla, usa el magnet (infohash + trackers) y emite la fase `metadata` mientras resuelve.
- `start_stream` es idempotente: si el torrent ya está activo (por ejemplo, porque se está descargando), devuelve la misma sesión.
- `stop_stream` pausa el torrent cuando es solo de streaming. Si además es una descarga, la descarga sigue. La caché se limpia más tarde, por LRU.
- `open_external_player` lanza el reproductor de Ajustes (VLC por defecto) con `streamUrl`. Si no está instalado, devuelve el error `external_player_missing`.
- Subtítulos para el reproductor externo (se pasan como archivo local, p. ej. `--sub-file=<ruta>` en VLC, `--sub-file=` en mpv):
  - `subtitleId`: subtítulo de OpenSubtitles ya elegido; se usa el `.vtt` de la caché en disco o se descarga.
  - `subtitlePath`: archivo propio (`.srt`/`.vtt`) que cargó el usuario.
  - Si no llega ninguno y `autoSubtitles` está activo con key configurada, el backend busca en `subtitleLang` y usa el primero del ranking (con el `infohash` para la coincidencia de release).
  - `subtitleDelayMs` se traduce a la opción de retraso del reproductor si la tiene (convención del front: positivo = los subtítulos salen más tarde).
  - Un fallo de subtítulos **nunca** impide abrir el reproductor: se abre sin ellos y se informa en el resultado.
  - Reproductor no reconocido (ni VLC ni mpv): se abre sin subtítulos con `subtitle: "unsupported_player"`.

```ts
type ExternalPlayerResult = {
  subtitle: "loaded" | "none" | "no_key" | "quota" | "not_found" | "unsupported_player" | "error";
  // "none" = no se pidió y la carga automática está apagada
};
```
- El progreso **no** lo guarda el backend por su cuenta: el front llama a `save_progress`.

### Subtítulos

| Comando | Argumentos | Devuelve |
|---|---|---|
| `search_subtitles` | `{ movieId: number, lang: string, infohash?: string }` | `SubtitleOption[]` |
| `load_subtitle` | `{ subtitleId: string }` | `SubtitleTrack` |
| `load_subtitle_file` | `{ path: string }` | `SubtitleTrack` |
| `get_subtitles_status` | — | `SubtitlesStatus` |

```ts
type SubtitleOption = {
  id: string;                      // id de OpenSubtitles (file_id)
  lang: string;                    // ISO 639-1: "es", "en"
  label: string;                   // nombre del release
  downloads: number;
  hearingImpaired: boolean;
  matchesRelease: boolean;         // coincide con el release de YTS del infohash dado
  aiTranslated: boolean;           // traducido por IA o por máquina
};

type SubtitleTrack = {
  trackUrl: string;                // .vtt servido por el servidor local (ya convertido de SRT y en UTF-8)
  lang: string | null;
  label: string;
};

type SubtitlesStatus = {
  configured: boolean;             // hay API key
  loggedIn: boolean;               // hay usuario y contraseña y el login funcionó
  remainingDownloads: number | null; // null si OpenSubtitles no lo informa (sin login)
  resetAt: string | null;          // ISO 8601: cuándo se renueva el cupo (último valor conocido)
};
```

- Los resultados se ordenan primero por `matchesRelease`, después dejando al final los `hearingImpaired` y `aiTranslated` (salvo que no haya otros), y por último por `downloads`, de mayor a menor.
- Si no hay API key, `search_subtitles` y `load_subtitle` fallan con `subtitles_auth`; `load_subtitle_file` funciona siempre.
- `load_subtitle` primero busca el `.vtt` en la caché de disco (`<datos>/subs/<fileId>.vtt`): si ya está, no llama a la API ni gasta cupo.
- `subtitles_quota`: el `message` lleva la hora de renovación; el front la obtiene de forma estructurada con `get_subtitles_status` (`resetAt`).
- `get_subtitles_status` valida la API key (y hace login si hay credenciales); con la key inválida falla con `subtitles_auth`. Es el botón "Probar" de Ajustes.
- `load_subtitle_file` acepta `.srt` y `.vtt`; la ruta llega del diálogo de archivo o del drag & drop de Tauri (`onDragDropEvent`, que da rutas reales).
- La búsqueda usa el `imdbCode` de la película: el backend lo obtiene a partir de `movieId`.
- El retraso de los subtítulos lo aplica el front, desplazando los `cue`. No pasa por IPC.

### Mi lista

| Comando | Argumentos | Devuelve |
|---|---|---|
| `list_favorites` | — | `MovieSummary[]` (las más recientes primero) |
| `add_favorite` | `{ movie: MovieSummary }` | `void` |
| `remove_favorite` | `{ movieId: number }` | `void` |

`add_favorite` recibe el `MovieSummary` completo para que "Mi lista" se pueda mostrar sin conexión.

### Continuar viendo

| Comando | Argumentos | Devuelve |
|---|---|---|
| `save_progress` | `{ movie: MovieSummary, positionS: number, durationS: number }` | `Progress` |
| `get_progress` | `{ movieId: number }` | `Progress \| null` |
| `list_continue_watching` | — | `ContinueItem[]` |
| `remove_progress` | `{ movieId: number }` | `void` |

```ts
type ContinueItem = { movie: MovieSummary; progress: Progress };
```

- El front llama a `save_progress` cada 10 s, al pausar y al salir del reproductor.
- `list_continue_watching` excluye las películas con `finished: true` y ordena por `updatedAt`, de más reciente a más antigua.

### Descargas

| Comando | Argumentos | Devuelve |
|---|---|---|
| `start_download` | `{ movie: MovieSummary, infohash: string }` | `Download` |
| `list_downloads` | — | `Download[]` |
| `pause_download` | `{ infohash: string }` | `Download` |
| `resume_download` | `{ infohash: string }` | `Download` |
| `remove_download` | `{ infohash: string, deleteFiles: boolean }` | `void` |
| `open_download_folder` | `{ infohash: string }` | `void` |

```ts
type DownloadState = "queued" | "active" | "paused" | "stalled" | "done" | "error";

type Download = {
  infohash: string;
  movie: MovieSummary;
  quality: Quality;
  videoCodec: VideoCodec;
  state: DownloadState;
  progress: number;                // 0–1
  sizeBytes: number;
  downloadedBytes: number;
  downSpeedBps: number;
  peers: number;
  etaS: number | null;
  path: string | null;             // carpeta en library/, cuando existe
  error: string | null;
  addedAt: string;
};
```

Si se llama a `start_download` sobre un torrent que ya se está reproduciendo, se **promueve**: deja de ser caché y pasa a `library/`, sin volver a descargar lo que ya se bajó.

### Ajustes y almacenamiento

| Comando | Argumentos | Devuelve |
|---|---|---|
| `get_settings` | — | `Settings` |
| `update_settings` | `{ patch: SettingsPatch }` | `Settings` |
| `get_storage_usage` | — | `StorageUsage` |
| `clear_cache` | — | `ClearCacheResult` |

```ts
type Settings = {
  // Catálogo
  apiBaseUrls: string[];           // en orden de preferencia
  // Subtítulos
  openSubtitlesApiKey: string | null;
  openSubtitlesUsername: string | null; // opcional: con cuenta, más cupo diario
  openSubtitlesPassword: string | null;
  subtitleLang: string;            // "es"
  autoSubtitles: boolean;
  // Reproducción
  preferredQuality: Quality;       // "1080p"
  preferX264: boolean;
  externalPlayer: string;          // comando, por defecto "vlc"
  bufferTargetBytes: number;
  // Torrent
  downLimitKbps: number | null;    // null = sin límite
  upLimitKbps: number | null;
  seedAfterDownload: boolean;
  listenPort: number | null;       // null = automático
  // Almacenamiento
  dataDir: string;
  cacheLimitBytes: number;
};

type StorageUsage = { cacheBytes: number; cacheLimitBytes: number; libraryBytes: number; freeDiskBytes: number };

type ClearCacheResult = { freedBytes: number };

// Todas las claves son opcionales. Clave ausente = no tocar.
// En los campos que admiten null (openSubtitlesApiKey, openSubtitlesUsername, openSubtitlesPassword, downLimitKbps, upLimitKbps, listenPort),
// enviar null = borrar el valor (sin key, sin límite, puerto automático).
// En Rust: Option<Option<T>> (p. ej. con serde_with::rust::double_option).
type SettingsPatch = Partial<Settings>;
```

- `update_settings` valida los datos y devuelve los ajustes completos ya aplicados. Los límites de velocidad y el puerto se aplican en caliente, sin reiniciar la app.
- `dataDir` es la excepción: el cambio se guarda, pero **se aplica al reiniciar la app**, y lo que hay en `cache/` y `library/` no se mueve solo. La UI tiene que avisarlo ("Se aplicará al reiniciar; las descargas existentes se quedan en la carpeta anterior").
- `openSubtitlesApiKey`, `openSubtitlesUsername` y `openSubtitlesPassword` solo se guardan en local (SQLite) y no se escriben nunca en los logs. Cambiarlos invalida el token de sesión de OpenSubtitles.

### Tráiler

| Comando | Argumentos | Devuelve |
|---|---|---|
| `open_trailer_window` | `{ ytTrailerCode: string, title: string }` | `void` |

Es el plan B por si el embed `youtube-nocookie` falla dentro de la WebView: abre una `WebviewWindow` aparte que carga la URL del embed directamente. Lo normal es que el front intente primero el modal con el iframe.

## Eventos (backend → frontend)

Se escuchan con `listen(evento, handler)` de `@tauri-apps/api/event`.

### `torrent://stats`
Se emite **cada segundo** por cada torrent activo, sea de streaming o de descarga.

```ts
type TorrentStats = {
  infohash: string;
  phase: StreamPhase;
  peers: number;
  seeds: number;                   // seeds según YTS/tracker para ese torrent (estático, el mismo de get_movie): librqbit no expone qué peers conectados son seeds
  downSpeedBps: number;
  upSpeedBps: number;
  progress: number;                // 0–1, todo el archivo
  downloadedBytes: number;
  bufferedAheadBytes: number;      // contiguos desde la posición de lectura actual
  availableRanges: [number, number][]; // fracciones 0–1 ya en disco (barra de progreso: "saltar ahí es inmediato")
  pieceMap: string | null;         // solo mientras hay un stream abierto. 200 celdas sobre la VENTANA pieceMapWindow (no sobre el archivo entero): "0" falta, "1" lista, "2" prioritaria (falta y está dentro de la ventana de ~32 MB que librqbit prioriza), "3" llegando (reservado, hoy no se emite)
  pieceMapWindow: { startByte: number; endByte: number } | null; // ventana del pieceMap: 64 MB desde la posición de lectura actual (antes de la primera lectura, desde resumeAtS o el byte 0); si el archivo es más chico, el archivo entero. ~330 KB por celda: el llenado del búfer de 8 MB se ve en ~25 celdas. El progreso global va en progress/availableRanges
};

type StreamPhase =
  | "connecting"   // "Conectando al enjambre…" (todavía sin peers conectados)
  | "metadata"     // solo si hubo que caer al magnet: resolviendo metadatos (sin peers visibles mientras tanto)
  | "buffering"    // por debajo de bufferTargetBytes
  | "ready"        // se puede reproducir
  | "stalled"      // sin peers o velocidad 0 durante más de 30 s
  | "seeding"
  | "done";
```

### `download://changed`
Se emite **solo cuando cambia el estado** de una descarga (agregada, pausada, terminada, error o eliminada). El progreso continuo llega por `torrent://stats`.

```ts
type DownloadChanged = { infohash: string; download: Download | null }; // null = eliminada
```

### `app://error`
Errores en segundo plano que no responden a ningún comando, como que se caiga el servidor local o que el disco se llene durante una descarga.

```ts
type BackgroundError = AppError & { infohash: string | null };
```

## Servidor HTTP local

Lo usa el front de forma indirecta, a través de las URLs que recibe. Se documenta aquí para depurar:

| Ruta | Contenido |
|---|---|
| `GET /stream/<infohash>/<fileIdx>` | Video con soporte de `Range` (`206 Partial Content`) |
| `GET /img/<hash-de-url>` | Imagen de YTS cacheada en disco (se descarga la primera vez que se pide) |
| `GET /subs/<id>.vtt` | Subtítulo convertido a VTT |

Solo escucha en `127.0.0.1`, en un puerto aleatorio que se elige al arrancar. Responde con `Access-Control-Allow-Origin` para el origen de la WebView.

## Cambios
- **v0** (2026-10-03): borrador inicial del MVP.
- **v0.1** (2026-10-03): reglas de opcionales separadas para entradas y salidas, `SettingsPatch` (null = borrar, ausente = no tocar), tipo `ClearCacheResult` con nombre, `dataDir` se aplica al reiniciar, `Torrent.source` desconocido → `"web"`.
- **v0.2** (2026-10-03): `genres` mantiene las mayúsculas de la API y el filtro va en minúsculas; lista fija de géneros en el front. `Download` no lleva seeds: la salud del enjambre sale de `TorrentStats.seeds`.
- **v0.3** (2026-10-03): `coverUrl`/`coverLargeUrl` pasan a `string | null`. Las URLs locales valen solo para la sesión y se reescriben al leer de la DB.
- **v0.4** (2026-10-03): `MovieSummary.maxSeeds` y `MovieDetail.screenshotUrls`.
- **v0.5** (2026-10-03): `TorrentStats.seeds` = seeds de YTS (estático); pieceMap "3" reservado y sin emitir; `start_stream` usa el `.torrent` de YTS y cae al magnet si falla (la fase `metadata` solo aparece en ese caso).
- **v0.6** (2026-10-03): `pieceMap` se muestrea sobre una ventana de 64 MB desde la posición de lectura; nuevo `TorrentStats.pieceMapWindow`.
- **v0.7** (2026-10-04): subtítulos. `SubtitleOption.aiTranslated`, comando `get_subtitles_status` (tipo `SubtitlesStatus`), credenciales opcionales `openSubtitlesUsername`/`openSubtitlesPassword` en `Settings`, caché de `.vtt` en disco y reglas de orden. Sin cambios que rompan.
- **v0.8** (2026-10-04): `open_external_player` pasa subtítulos al reproductor externo (`subtitleId?`, `subtitlePath?`, `subtitleDelayMs?`) y devuelve `ExternalPlayerResult` en vez de `void`.
