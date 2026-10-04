# YTS Player — "Netflix de YTS" para escritorio

## Contexto
Hoy las películas se descargan a mano desde yts.gg. La idea es una app de escritorio (Linux primero) que muestre el catálogo de YTS con una interfaz tipo Netflix y reproduzca las películas **en streaming desde el torrent**, mientras se descargan, con opción de guardarlas, subtítulos en español automáticos, trailers, "Mi lista" y "Continuar viendo".

Decisiones tomadas: **Tauri 2 + React + TypeScript + Rust**, streaming + descarga, subtítulos ES automáticos y el MVP con las 4 funciones.

Hechos verificados:
- La API v2 de YTS responde en `https://yts.gg/api/v2/` (`list_movies.json`, `movie_details.json`, ~77.5k películas, con `torrents[]` por calidad y `hash`, `imdb_code`, `yt_trailer_code` y las portadas). El aviso de la propia API dice que **la URL base se muda a `https://movies-api.accel.li/api/v2/`**, así que la URL base tiene que ser configurable y tener respaldo.
- Versiones actuales: tauri 2.12, librqbit 9.0 y rusqlite 0.40.
- El sistema ya tiene `gstreamer1.0-libav` y `plugins-bad`, así que la WebView (WebKitGTK) puede reproducir H.264/AAC en `<video>`. VLC está instalado y sirve como alternativa.
- El directorio del proyecto está vacío (proyecto nuevo).

Nota legal: la app es solo un cliente. El contenido de YTS suele tener copyright y su legalidad depende del país, así que el uso queda bajo tu responsabilidad.

## Arquitectura

```
React UI (WebView)  ──invoke/events──►  Rust core (Tauri)
  catálogo, ficha, player                ├─ yts_client   (reqwest → API YTS, caché)
  <video src="http://127.0.0.1:PORT/…">  ├─ torrent      (librqbit: sesión, prioridad secuencial)
                                         ├─ stream_server (HTTP local con Range → archivo del torrent)
                                         ├─ subtitles    (OpenSubtitles → .vtt)
                                         └─ db           (SQLite: favoritos, progreso, ajustes)
```

Clave del streaming: librqbit ya expone el contenido de un torrent como un stream con soporte para `Range` y le da prioridad a las piezas que se están leyendo. El `<video>` de la WebView apunta a `http://127.0.0.1:<puerto>/stream/<infohash>/<fileIdx>`, y al adelantar el video se envía un `Range` y librqbit cambia la prioridad de las piezas.

## Estructura del proyecto
```
yts-movie-player/
├─ src/                        # React + TS (Vite)
│  ├─ api/tauri.ts             # wrappers tipados de invoke()
│  ├─ routes/ (TanStack Router, basado en archivos) __root, index, search, movie.$movieId, play.$movieId, my-list, downloads, settings
│  ├─ components/ MovieRow.tsx, MovieCard.tsx, HeroBanner.tsx, Filters.tsx, TrailerModal.tsx, PlayerControls.tsx
│  └─ store/                   # TanStack Query (datos remotos) + Zustand (estado UI)
└─ src-tauri/src/
   ├─ main.rs / lib.rs         # setup, registro de comandos y estado compartido
   ├─ yts.rs                   # cliente API + modelos serde
   ├─ torrent.rs               # sesión librqbit, start/stop/stats
   ├─ stream.rs                # servidor HTTP local (axum) o el servidor HTTP propio de librqbit
   ├─ subtitles.rs             # búsqueda/descarga + conversión SRT→VTT
   ├─ db.rs                    # rusqlite + migraciones
   └─ commands.rs              # #[tauri::command]
```

## Componentes en detalle

### 1. Cliente YTS (`yts.rs`)
- `list_movies(page, limit=50, query_term, genre, quality, minimum_rating, sort_by, order_by)` y `movie_details(id, with_images, with_cast)`, más `movie_suggestions(id)` para "Similares".
- Lista de URLs base `[movies-api.accel.li, yts.gg]` con failover automático y un override en los ajustes.
- Caché en memoria/SQLite con TTL de unos 30 minutos. Las imágenes se cachean en disco para que el scroll vaya fluido.
- Se arma el magnet con `hash` + trackers públicos (la lista de trackers que recomienda YTS).

### 2. Motor torrent (`torrent.rs`, librqbit)
- Una `Session` global con carpeta de datos (`~/.local/share/yts-player/`): `cache/` para el streaming y `library/` para las descargas guardadas.
- `start_stream(movie_id, quality)` agrega el magnet, elige el archivo de video más grande y devuelve la URL del stream.
- Se emite el evento `torrent://stats` cada segundo (velocidad, peers, % y buffer listo).
- **Modo streaming**: al cerrar el reproductor se pausa y luego se limpia la caché (límite configurable en GB).
- **Modo descarga**: "Descargar" mueve o marca el torrent como persistente en `library/`. Si una película ya está descargada, se reproduce desde el archivo local.
- Ajustes: límite de subida/bajada, seguir compartiendo (seed) o no, y puerto.

### 3. Reproductor (`Player.tsx`)
- `<video>` HTML5 con controles propios: play/pausa, barra de progreso que muestra lo que hay en buffer, volumen, pantalla completa, atajos (espacio, ←/→ ±10 s, F, M), selector de subtítulos y retraso de subtítulos.
- Pantalla de carga con peers y velocidad hasta que haya unos 5–10 MB en buffer.
- Alternativa: si el `<video>` da error de códec (por ejemplo x265/HEVC), aparece el botón **"Abrir en VLC"**, que lanza `vlc <url-stream>`. En la ficha se prefieren los torrents x264 cuando hay varios de la misma calidad.
- El progreso se guarda cada 10 s y al salir. Pasado el 92 % se marca como vista.

### 4. Subtítulos (`subtitles.rs`)
- API REST de OpenSubtitles (`api.opensubtitles.com`, necesita una API key gratuita que se pone en Ajustes) con búsqueda por `imdb_id` + `languages=es`, y se ordena por descargas y coincidencia con el "release" de YTS.
- Se convierte SRT→VTT (con detección de encoding: latin-1 o utf-8) y se sirve desde el mismo servidor local como `<track>`.
- Idioma por defecto ES, con opción de elegir otro (EN) y de cargar un `.srt` manual (arrastrar y soltar).

### 5. Persistencia (`db.rs`, SQLite)
- `favorites(movie_id, added_at, movie_json)`
- `progress(movie_id, position_s, duration_s, updated_at, finished)`
- `downloads(movie_id, infohash, quality, path, status)`
- `settings(key, value)`

### 6. UI tipo Netflix
- **Home**: un hero banner (película destacada con `background_image`), seguido de las filas "Continuar viendo", "Mi lista", "Tendencias" (`sort_by=download_count`), "Recientes" (`date_added`), "Mejor valoradas" (`rating`) y una fila por género, con scroll horizontal infinito.
- **Buscar**: con debounce y filtros de género, calidad (720p/1080p/2160p), año, rating mínimo y orden.
- **Ficha**: fondo, sinopsis, rating IMDb, duración, reparto, botones ▶ Reproducir / Continuar, selector de calidad (tamaño, peers y seeds), ♥ Mi lista, ⬇ Descargar y ▶ Tráiler, más "Similares".
- **Tráiler**: modal con `youtube-nocookie.com/embed/<yt_trailer_code>`. Riesgo conocido: YouTube puede rechazar el embed desde el origen `tauri://`. En ese caso el plan B es abrir una `WebviewWindow` aparte cargando la URL del embed directamente.
- **Descargas**: lista con progreso, pausar/reanudar/borrar y "Abrir carpeta".
- Tema oscuro, navegable con teclado, tarjetas con hover y una animación de escala.

## Fases de implementación
> **Sustituido por [`ROADMAP.md`](ROADMAP.md)**, que tiene el detalle por agente, los tests y los criterios de cierre (y cambia el orden: persistencia antes que subtítulos). Esta lista se conserva como referencia.
1. **Base**: `npm create tauri-app` (React-TS, Vite) + Tailwind, router y TanStack Query. Instalar `libwebkit2gtk-4.1-dev`, `build-essential`, `libssl-dev`, `librsvg2-dev` y `libayatana-appindicator3-dev`.
2. **Catálogo**: `yts.rs`, comandos, Home con filas, Búsqueda/Filtros, Ficha y caché de imágenes.
3. **Streaming**: librqbit + servidor local + Player básico. **Este es el hito crítico**, hay que validarlo antes de seguir.
4. **Subtítulos**: OpenSubtitles + VTT + selector.
5. **Persistencia**: SQLite, Mi lista y Continuar viendo (con retomar en el segundo exacto).
6. **Descargas** y gestión de caché, más Ajustes.
7. **Trailers** y pulido (atajos, estados de error, sin conexión o sin peers).
8. **Empaquetado**: `tauri build` en `.deb`/AppImage (y más adelante Windows/macOS).

## Riesgos y mitigaciones
| Riesgo | Mitigación |
|---|---|
| Cambia o se cae el dominio de la API de YTS | URLs base configurables + failover |
| Códecs que no soporta WebKitGTK (HEVC) | Preferir x264, botón "Abrir en VLC" (a futuro: integrar libmpv) |
| Torrent con pocos peers | Mostrar seeds en el selector de calidad y un aviso de "pocos seeds" |
| Bloqueo del embed de YouTube | Ventana webview aparte o abrir en el navegador |
| Disco lleno por la caché | Límite de caché con limpieza LRU |

## Verificación
- `cargo test` en `src-tauri`: parseo de respuestas reales de la API (fixtures JSON), armado del magnet, SRT→VTT y migraciones de la DB.
- `npm run tauri dev`: navegar por la Home, buscar "Matrix", abrir la ficha, reproducir 1080p y comprobar que empieza en menos de 30 s, que al adelantar con la barra (`Range`) sigue reproduciendo y que los subtítulos ES aparecen sincronizados.
- Cerrar a mitad de la película, volver a abrir la app y verificar que está en "Continuar viendo" y que retoma desde el mismo punto.
- Agregar a Mi lista y reiniciar la app (debe persistir). Descargar una película, desconectar la red y reproducirla desde la biblioteca.
- `curl -r 0-1000 http://127.0.0.1:PORT/stream/...` debe devolver `206 Partial Content`.

## Decisiones y lecciones registradas durante el desarrollo
- **Ubicación de los datos:** la DB (`yts-player.db`), `dht.json` y los ajustes viven siempre en `~/.local/share/yts-player/`. Las carpetas de caché y de descargas se eligen aparte (`Settings.cacheDir` y `Settings.downloadsDir`, en caliente), porque la DB tiene que estar en un lugar fijo para poder leer los ajustes. Cada descarga guarda su propia ruta, así que cambiar `downloadsDir` no rompe las existentes; moverlas es una acción explícita (`move_downloads`).
- **Caché:** una carpeta por torrent (`cache/<infohash>/`), LRU por mtime contando los bytes **asignados** (los archivos de librqbit son *sparse*). Nunca toca streams abiertos ni con lectores (VLC), ni `cache/img`, ni `library/`.
- **Espacio en disco (fase 4):** librqbit mantiene abiertos los archivos de un torrent pausado. Si se borran con `rm`, desaparecen de `du` pero `df` no libera el espacio hasta que el proceso termina. Por eso toda limpieza **primero saca el torrent de la sesión** (`TorrentEngine::evict`) y después borra. Hay un test que comprueba en `/proc/self/fd` que no quede nada abierto.
- **WebKitGTK (fase 3):** el `<video>` no siempre dispara `playing` al recuperarse de una espera, un seek o una pausa. El estado del reproductor se deriva de `video.paused`/`currentTime` y de `canplay`/`seeked`/`timeupdate`.
- **librqbit no reconecta a un peer perdido** si no hay DHT ni trackers que den otros (visto como un test intermitente: `stalled` al 98,9 % tras pausar y reanudar con un solo peer local). Mitigación: una descarga activa que queda `stalled` con 0 peers se pausa y reanuda una vez por minuto, lo que vuelve a anunciar y reintenta los peers conocidos.
- **Carpetas configuradas que faltan:** al arrancar o en la comprobación periódica no se crean (para no escribir dentro de un punto de montaje vacío). La caché cae a la carpeta por defecto y las descargas de esa carpeta quedan `unavailable`. Solo se crean al elegirlas en Ajustes.
- **Tráiler de YouTube:** el embed da el error 153 si no llega un `Referer` válido, y `tauri://localhost` no lo manda (ni en una ventana aparte ni en un iframe con `no-referrer`). Funciona dentro de una página servida desde `http://127.0.0.1` (`/trailer/<code>`), que la app mete en un iframe. **Pendiente para Windows (fase 8):** allí el origen es `https://tauri.localhost` y WebKit bloqueó el iframe http como contenido mixto; habrá que probar WebView2 o ir directo a la ventana aparte.
- **Rendimiento (fase 7, release):** 4 descargas + 1 stream con 12 MiB/s en total → RSS 22,6 MiB máx, CPU media 43 % de un núcleo (sobre todo el hash de piezas), 0 tareas huérfanas tras el cierre. Cierre ordenado en ~1,4 s (máximo 5 s). Repetir: `cargo build --release --example e2e_seeder && cargo test --release --test perf -- --ignored --nocapture`.
