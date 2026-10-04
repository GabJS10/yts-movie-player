# `data-testid` y modo mocks (para los E2E)

Contrato estable entre el frontend y los E2E (`e2e/`). Renombrar o quitar un id es un cambio que rompe: se avisa a `plan`. Los ids no dependen del idioma ni del contenido; cuando un elemento se repite, lleva atributos `data-*` para elegir uno.

## Arrancar la interfaz con mocks

```bash
npm run dev            # Vite en http://localhost:1420 (puerto fijo, strictPort)
```

Fuera de Tauri (un navegador normal, Playwright incluido) y en modo desarrollo, `src/main.tsx` instala el backend simulado de `src/mocks/` (nunca llega a un build de producción). Datos: `src/mocks/catalog.json`. Las imágenes salen de `/design/prototype/...` (las sirve el propio Vite). El video de prueba del reproductor es un MP4 H.264 externo (`MOCK_H264_URL` en `src/mocks/backend.ts`); sin internet el `<video>` no carga, pero el resto del reproductor sí.

### Escenarios: por URL o por localStorage

Cada opción se puede pasar en la URL de la primera carga (se lee una vez al arrancar, gana sobre localStorage y se quita de la URL antes de que la vea el router) o en `localStorage` con la misma clave (persiste entre recargas; en Playwright, con `page.addInitScript`).

| Clave | Valor | Efecto |
|---|---|---|
| `mock:latency` | ms (`0` = inmediato) | Latencia fija de cada comando. Sin la clave: 150–450 ms al azar. Para E2E conviene `0`. |
| `mock:fail` | `cmd[=code],…` | Esos comandos rechazan con ese `ErrorCode` (por defecto `db`). Ej.: `get_movie=network`, `start_stream=no_peers,add_favorite`. |
| `mock:offline` | `1` | Sin red: el catálogo falla con `network`; las películas descargadas abren su copia guardada y se reproducen desde la biblioteca. |
| `mock:tick` | ms (`0` = nunca) | Paso de la simulación (búfer del stream, descargas, mover). Por defecto 1000. Con `0` se avanza a mano. |
| `mock:trailer` | `fake` \| código | Página del tráiler: `fake` responde `ready`/`playing` sin YouTube (sin internet); un número (p. ej. `153`) manda ese error y dispara la cadena (ventana → navegador). Sin la clave: el embed real de YouTube. |
| `mock:update` | versión | `check_for_update` responde esa versión (p. ej. `1.1.0`) como nueva; sin la clave, la app está al día (la del mock es `1.0.0`). |
| `mock:settings` | JSON | Parche de ajustes antes del primer render. Ej.: `{"openSubtitlesApiKey":"quota"}`, `{"openSubtitlesApiKey":null}`. |

Ejemplo: `http://localhost:1420/movie/1632?mock:latency=0&mock:fail=start_stream=no_peers`.

En la consola (y desde `page.evaluate`) está `window.__ytsMock`:
- `__ytsMock.tick()`: avanza un paso de la simulación y emite `torrent://stats`.
- `__ytsMock.backend.handle(cmd, args)`: llama al backend simulado directamente (p. ej. `pause_download`).
- `__ytsMock.backend.setOffline(true|false)`, `.completeDownload(infohash)` y `.patchDownload(infohash, { state: "unavailable" })`.

### Datos y valores mágicos del mock

- **Errores:**
  - Buscar `!api` da `api_unavailable` y `!net` da `network`.
  - `/movie/1` da `not_found`.
- **Descargas iniciales:**
  - Descargando: The Dark Knight (3175, 1080p) y Coco (7062).
  - En pausa: The Shawshank Redemption (3709, 720p).
  - Sin seeds: Spider-Verse (10960, 2160p).
  - Terminada: The Godfather (3304, 1080p).
- **Mi lista:** 3175, 3304, 3709, 7062 y 1632.
- **Continuar viendo:** 10960, 7062 y 8462.
- **Subtítulos:**
  - Clave `invalid` da `subtitles_auth`; clave `quota` da cupo agotado.
  - The Shawshank Redemption no tiene subtítulos en español.
- **Versiones con 3–4 seeds:** se quedan en `stalled`. **Con 2 o menos** (p. ej. Captain Marvel 3D, id 12176) nunca conectan y a los 10 pasos (`NO_PEERS_TICKS`; 60 s en la app real) pasan a `no_peers`.
- **Banner:** `get_featured` del mock es estable en la sesión: sugerencias de Spider-Verse (último visto), de Mi lista, un género y tendencias. Para el banner de siempre: `mock:fail=get_featured`.
- **Carpetas:**
  - "Cambiar…" elige `/media/usb/Películas`.
  - Una ruta con `sin-permiso` se rechaza; con `desconectado` sale como no disponible; con `lleno` no se pueden mover las descargas de más de 2 GB.

## Lista de `data-testid`

### Navegación (`AppShell`)
| id | Elemento |
|---|---|
| `nav-home`, `nav-search`, `nav-my-list`, `nav-downloads` | Enlaces de la barra superior (ancho > 900 px). El activo lleva `aria-current="page"`. |
| `tab-home`, `tab-search`, `tab-my-list`, `tab-downloads`, `tab-settings` | Barra inferior compacta (≤ 900 px). |
| `nav-search-icon`, `nav-downloads-icon`, `nav-settings-icon` | Iconos de la derecha. |
| `nav-downloads-badge` | Contador de descargas activas (solo si hay alguna). |
| `offline-indicator` | Pastilla "Sin conexión" (solo sin red). |
| `toast` | Aviso flotante; `data-tone="ok" \| "error"`. |

### Inicio y tarjetas
| id | Elemento |
|---|---|
| `hero` | Banner destacado. |
| `hero-title`, `hero-play`, `hero-info`, `hero-favorite` | Título, Reproducir, Más info y ♥ del banner. |
| `hero-prev`, `hero-next`, `hero-pause` | Controles del banner rotativo (`hero-pause` lleva `aria-pressed`; oculto con `prefers-reduced-motion`). |
| `hero-dot` | Punto de cada destacada; `data-index` y `aria-current="true"` en la actual. El `hero` lleva `data-index` de la actual. |
| `hero-reason` | Motivo ("Porque viste X", "Para ti: Acción"…). Sin recomendaciones no aparece (banner de siempre). |
| `movie-row` | Fila de películas; `data-title` = título visible de la fila. |
| `movie-card` | Tarjeta (enlace a la Ficha); `data-movie-id`. |
| `card-in-list` | Corazón verde de la tarjeta si la película está en Mi lista. |

### Buscar
| id | Elemento |
|---|---|
| `search-input` | Caja de búsqueda. |
| `filter-genre`, `filter-sort` | `<select>` de género y orden. |
| `filter-quality`, `filter-rating` | Grupos de botones (`aria-pressed`). |

### Ficha (`/movie/:id`)
| id | Elemento |
|---|---|
| `movie-title` | Título (h1). |
| `movie-play` | Reproducir / Continuar (enlace a `/play/:id?infohash=…`). |
| `movie-restart` | "Desde el principio" (solo con progreso). |
| `movie-favorite` | Mi lista (`aria-pressed`). |
| `movie-download` | Descargar; `data-state` = `none` (botón) o el `DownloadState` (enlace a Descargas). |
| `movie-trailer` | Tráiler (solo con `ytTrailerCode` y con red). |
| `trailer-dialog`, `trailer-frame`, `trailer-youtube`, `trailer-close` | Modal del tráiler; `trailer-frame` lleva `data-phase` (`loading`, `ok`). |
| `version-row` | Fila de la tabla de versiones (`role="radio"`, `aria-checked`); `data-infohash`. |

### Reproductor (`/play/:id`)
| id | Elemento |
|---|---|
| `video` | El `<video>`. |
| `player-play-toggle` | Play / pausa. |
| `player-scrub` | Barra de progreso (`role="slider"`, flechas ←/→); `scrub-rail` es su carril interno. |
| `player-back-10`, `player-forward-10`, `player-fullscreen`, `player-back` | −10 s, +10 s, pantalla completa y salir. |
| `clock` | Tiempo actual / total. |
| `phase` | Fase del arranque en la pantalla de búfer (`connecting`, `buffering`…). |
| `no-peers`, `no-peers-alternative`, `no-peers-wait`, `no-peers-back` | Pantalla `no_peers`: otra versión con más seeds, seguir esperando, volver. |
| `piece-map`, `subtitles`, `subtitle-notice`, `quota-line` | Mapa de piezas, capa de subtítulos y avisos de subtítulos. |

### Descargas (`/downloads`)
| id | Elemento |
|---|---|
| `download-row` | Una descarga; `data-infohash`, `data-state` (`DownloadState`). |
| `download-progress` | Barra (`role="progressbar"`, `aria-valuenow` 0–100). |
| `download-play` | Reproducir (solo completas). |
| `download-toggle` | Pausar / Reanudar (las no completas). |
| `download-folder`, `download-remove` | Abrir carpeta y Quitar. |
| `remove-dialog`, `remove-cancel`, `remove-keep`, `remove-delete` | Diálogo de quitar: Cancelar, Conservar archivos, Borrar archivos. |

### Ajustes
| id | Elemento |
|---|---|
| `move-dialog`, `move-cancel`, `move-background`, `move-close` | Diálogo de mover descargas. |
| `app-version`, `open-logs` | Acerca de: versión instalada y "Abrir carpeta de registros". |
| `update-notice`, `update-open`, `update-dismiss` | Aviso de nueva versión (abajo a la izquierda; una vez por sesión): Ver novedades y cerrar. |

### Errores
| id | Elemento |
|---|---|
| `error-state` | Bloque de error con acción; `data-code` = `ErrorCode`. |
| `error-retry`, `error-link`, `error-back` | Reintentar (si sirve para ese código), ir a donde se arregla (Ajustes, Descargas, Inicio) y volver (en el reproductor: "Volver a la ficha"). |
