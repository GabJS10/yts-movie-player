# Roadmap: fases de desarrollo

**Dueño:** `plan`. Describe **todas** las fases desde el estado actual hasta la versión 1.0: qué hace cada agente, qué se prueba y cuándo se da por terminada cada fase.

- Arquitectura y decisiones técnicas: [`PLAN.md`](PLAN.md)
- Contrato entre front y back: [`IPC.md`](IPC.md)
- Roles y reglas de coordinación: [`../AGENTS.md`](../AGENTS.md)

> El orden de las fases de este documento **sustituye** al de la sección "Fases de implementación" de `PLAN.md`. Cambia una cosa: la persistencia (SQLite + Ajustes) va antes que los subtítulos, porque la API key de OpenSubtitles se guarda en Ajustes.

## Resumen

| Fase | Nombre | Resultado visible | Estado |
|---|---|---|---|
| 0 | Planificación y diseño | Plan, contrato IPC y prototipo navegable | ✅ Terminada |
| 1 | Base del proyecto y tooling | La app abre una ventana con la estructura de navegación; CI en verde | ✅ Terminada (`fase-1`) |
| 2 | Catálogo | Home, Búsqueda y Ficha con datos reales de YTS | ✅ Terminada (`fase-2`) |
| 3 | Streaming (**hito crítico**) | Se reproduce una película desde el torrent y se puede adelantar | ✅ Terminada (`fase-3`) |
| 4 | Persistencia: Mi lista, Continuar viendo, Ajustes base y límite de caché | Favoritos y progreso que sobreviven a un reinicio; la caché no llena el disco | ✅ Terminada (`fase-4`) |
| 5 | Subtítulos | Subtítulos en español automáticos y sincronizados | ✅ Terminada (`fase-5`) |
| 6 | Descargas, caché y Ajustes completos | Descargar, ver sin conexión y gestionar el espacio | ✅ Terminada (`fase-6`) |
| 7 | Tráilers, pulido, robustez y E2E | Cada fallo tiene una salida; tests E2E en verde | ✅ Terminada (`fase-7`) |
| 8 | Empaquetado y release v1.0 | `.deb`, `.rpm` y AppImage publicados en GitHub Releases | ✅ Terminada (`v1.0.0`) |

## Cómo se trabaja cada fase

1. **Arranque (`plan`):** detalla la fase, el usuario la aprueba y `plan` reparte las tareas con `herdr agent prompt`.
2. **Trabajo en paralelo:** `backend` y `frontend` trabajan cada uno en su área (ver "Es dueño de" en `AGENTS.md`). Mientras el backend de esa fase no está listo, el frontend trabaja con **mocks tipados según `IPC.md`**.
3. **Sincronización:** cualquier cambio en el contrato pasa por `plan` y se registra en `IPC.md` (sección "Cambios").
4. **Integración (`plan`):** quita los mocks de esa fase, ejecuta la app de verdad, revisa los criterios de cierre y los tests y corrige lo que falte (o lo devuelve al agente correspondiente).
5. **Cierre:** el usuario da el visto bueno. Entonces se hace el commit de cierre, se crea el tag `fase-N`, se hace push y se actualiza la tabla de arriba.

### Git
- Rama `main` y un único árbol de trabajo. Cada agente hace `git add` **solo de sus propias rutas**, nunca `git add -A`.
- Los commits siguen Conventional Commits con un scope por agente: `feat(backend): …`, `feat(frontend): …`, `chore(ci): …`, `docs: …`.
- Los push y los tags los hace `plan` al cerrar cada fase o en hitos intermedios acordados.

### Definición de terminado (vale para todas las fases)
- `npm run lint`, `npm run typecheck` y `npm test` pasan.
- `cargo fmt --check`, `cargo clippy -- -D warnings` y `cargo test` pasan.
- El CI de GitHub Actions está en verde.
- La funcionalidad de la fase está verificada **en la app real** (`npm run tauri dev`), no solo con mocks.
- `IPC.md` coincide con el código y la documentación tocada está actualizada.

## Estrategia de tests

| Capa | Herramientas | Qué se prueba |
|---|---|---|
| Backend: unitarios | `cargo test` | Parseo de la API de YTS con **fixtures JSON reales** (`src-tauri/tests/fixtures/`), modelos serde tolerantes, armado de magnets, conversión SRT→VTT y detección de encoding, serialización de los tipos del IPC (camelCase, `null`, códigos de error) |
| Backend: integración | `cargo test` + `wiremock` + `tempfile` | Cliente YTS contra un servidor HTTP falso (failover, timeouts, caché). Migraciones SQLite en una DB temporal. Servidor de streaming: respuestas `Range` → `206`. Torrent de punta a punta: una sesión de librqbit hace de seeder de un archivo local y otra lo descarga y lo sirve por HTTP (sin internet) |
| Frontend: unitarios y componentes | Vitest + Testing Library + `@tauri-apps/api/mocks` (`mockIPC`) | Hooks de datos (TanStack Query), formateadores (tamaño, duración, velocidad), selección de torrent (preferir x264, mejor calidad con seeds), componentes (tarjeta, fila, filtros, controles del reproductor y atajos), conversión de `ErrorCode` a texto y acción |
| E2E | WebdriverIO + `tauri-driver` (Linux) | Flujos completos sobre la app compilada: navegar → ficha → reproducir (con torrent local de prueba) → progreso guardado → Mi lista (fase 7) |
| Manual | Lista de comprobación de cada fase | Lo que no se automatiza: torrents reales de YTS, códecs, rendimiento |

Regla: **todo bug corregido viene con un test que lo reproduce.**

---

## Fase 0: Planificación y diseño ✅

- **plan:** `PLAN.md`, `YTS-API.md`, `IPC.md`, `AGENTS.md`, el repo en GitHub y este roadmap.
- **frontend:** `DESIGN.md`, `PRODUCT.md` y el prototipo navegable en `design/prototype/`.
- **backend:** aún sin tareas.

## Fase 1: Base del proyecto y tooling ✅

**Objetivo:** un proyecto Tauri que compila, abre una ventana con la estructura de navegación de la app y tiene lint, tests y CI funcionando en las dos capas. Todavía sin funcionalidades.

- **plan:** instalar las dependencias del sistema, crear el scaffold con `create-tauri-app` (es compartido: crea `src/` y `src-tauri/` a la vez), los scripts de `package.json` y el CI de GitHub Actions (lint, typecheck, tests y cargo en Ubuntu).
- **backend:** reorganizar `src-tauri/` en módulos según `PLAN.md`, implementar `AppError` y los tipos del IPC con serde, la ruta del directorio de datos, el logging con `tracing`, `AppState` y los tests de serialización.
- **frontend:** Tailwind con los tokens de `DESIGN.md`, la fuente Archivo en local, TanStack Router (rutas basadas en archivos, con búsqueda y filtros tipados en los search params), el shell (barra de navegación) tal como en el prototipo, `src/api/types.ts` y `src/api/tauri.ts`, la capa de mocks, TanStack Query + Zustand, ESLint/Prettier y Vitest + Testing Library.
- **Tests:** la infraestructura en las dos capas, con al menos un test real por capa y el CI corriéndolos.
- **Cierre:** `npm run tauri dev` abre la ventana con el shell y se puede navegar entre las rutas (vacías). El CI está en verde. Tag `fase-1`.

## Fase 2: Catálogo ✅

**Objetivo:** recorrer el catálogo real de YTS con la interfaz tipo Netflix.

- **backend:**
  - `yts.rs`: cliente `reqwest` con la lista de URLs base y failover, timeouts y caché en memoria con TTL.
  - Modelos serde tolerantes (`is_repack`, falta de `movies`) y conversión a los tipos del IPC (`MovieSummary`, `MovieDetail`, `Torrent`).
  - Servidor HTTP local (axum) con la ruta `/img/<hash>` como caché de imágenes en disco.
  - Comandos `list_movies`, `get_movie`, `get_suggestions` y `get_api_status`.
- **frontend:**
  - Home: banner principal y filas Tendencias, Recientes, Mejor valoradas y por género, con scroll horizontal y carga incremental.
  - Búsqueda con debounce, filtros y orden.
  - Ficha: sinopsis, reparto, tabla de versiones con seeds y peers, señal de salud del enjambre y avisos de HEVC o pocos seeds, y la fila Similares.
  - Estados de carga (skeletons), vacío y error. Navegación con teclado.
  - Los botones Reproducir, Mi lista y Descargar aparecen pero todavía están desactivados.
- **Tests:**
  - Backend: fixtures reales (incluida una búsqueda sin resultados) y wiremock para el failover y los errores.
  - Frontend: componentes (tarjeta, fila, filtros), el hook `useMovies` con `mockIPC` y la selección de la mejor versión.
- **Cierre:** con la app real se busca "Matrix", se abre la ficha y se ven las versiones. Si la URL base primaria falla, la app pasa sola a la de respaldo. Tag `fase-2`.

## Fase 3: Streaming (hito crítico) ✅

**Resultado real:** 1080p x264 con 100 seeds → `.torrent` en 1,4 s, 8 MB de búfer en 9,0 s (~39 MB/s, 16 peers); adelantar funciona y HEVC → VLC. WebKitGTK + GStreamer reproduce H.264 sin problemas, así que la arquitectura queda validada. Lección: en WebKitGTK no hay que depender del evento `playing` para salir de pausa o espera.

**Objetivo:** pulsar Reproducir y ver la película en menos de 30 segundos, pudiendo adelantar.

- **backend:**
  - `torrent.rs`: sesión de librqbit, magnet con trackers y elección del archivo de video más grande.
  - `stream.rs`: ruta `/stream/<infohash>/<fileIdx>` con `Range` → `206` y prioridad de piezas según la posición de lectura.
  - Evento `torrent://stats` (fases, `availableRanges`, `pieceMap`).
  - Comandos `start_stream`, `stop_stream` y `open_external_player` (VLC).
  - Caché de streaming en `cache/`, con una limpieza básica al cerrar.
- **frontend:**
  - Pantalla de búfer como en el prototipo: fase, mapa de piezas, peers, velocidad y MB listos, con Enter para empezar ya.
  - Reproductor `<video>` con controles propios: barra de progreso con "visto", "en búfer" y "ya descargado", volumen, pantalla completa y atajos (espacio, ←/→, F, M).
  - Detección de error de códec, con la pantalla "Abrir en VLC" y la sugerencia de una versión x264.
- **Tests:**
  - Backend: de punta a punta con un torrent local (seeder y downloader en el test, sin internet), `Range` → `206`, selección del archivo y conversión a `StreamPhase`.
  - Frontend: controles y atajos del reproductor, conversión de `availableRanges` a la barra de progreso, flujo del error de códec.
- **Cierre:** una película 1080p x264 real empieza en menos de 30 s, se puede adelantar a cualquier punto y la 2160p x265 ofrece VLC. Esta fase **se valida antes de seguir**: si algo de la arquitectura falla (WebKitGTK, librqbit), se replantea aquí. Tag `fase-3`.

## Fase 4: Persistencia (Mi lista, Continuar viendo, Ajustes base y límite de caché) ✅

**Resultado real:** Mi lista y Continuar viendo sobreviven al reinicio, retomar y "Desde el principio" funcionan, los Ajustes se aplican en caliente y la caché respeta el límite (LRU de 10 GiB por defecto). La diferencia entre `df` y `du` venía de archivos borrados que librqbit seguía teniendo abiertos: ahora se saca el torrent de la sesión antes de borrar.

**Objetivo:** que la app recuerde las cosas entre sesiones y que la caché de streaming no llene el disco.

> El **límite de la caché** (LRU, `get_storage_usage`, `clear_cache`) se adelantó de la fase 6 a esta por pedido del usuario: probando el streaming se quedó sin disco dos veces.

- **backend:**
  - `db.rs`: rusqlite (feature `bundled`) en `~/.local/share/yts-player/yts-player.db` (siempre en la ubicación por defecto: `dataDir` solo mueve `cache/` y `library/`), migraciones versionadas y acceso a la DB fuera del hilo async (`spawn_blocking` o similar). Tablas `favorites`, `progress`, `settings`, `images` (registro hash → URL remota, para que las portadas guardadas sigan funcionando tras reiniciar) y `downloads` (se crea vacía; se usa en la fase 6).
  - Las películas guardadas (`MovieSummary`) se guardan **sin el origen** `http://127.0.0.1:<port>` y se reescriben con el puerto actual al leerlas (ver convenciones de `IPC.md`).
  - Comandos de Mi lista (`list_favorites`, `add_favorite`, `remove_favorite`) y de Continuar viendo (`save_progress`, `get_progress`, `list_continue_watching`, `remove_progress`). Se marca como vista (`finished`) a partir del 92 %.
  - `get_movie` devuelve `isFavorite` y `progress` reales; `start_stream` devuelve `resumeAtS` a partir del progreso guardado (null si está `finished`).
  - `get_settings` y `update_settings` (`SettingsPatch`: ausente = no tocar, null = borrar) con valores por defecto y validación. **Se aplican ya:** `apiBaseUrls` (en caliente en el cliente YTS), `preferredQuality`, `preferX264`, `externalPlayer`, `bufferTargetBytes` y `cacheLimitBytes`. Se guardan pero se aplican en la fase 6: límites de velocidad, puerto y `seedAfterDownload`; `dataDir` se aplica al reiniciar.
  - **Límite de caché (adelantado):** `cacheLimitBytes` por defecto 10 GB. Limpieza LRU (por último acceso) al arrancar, al abrir un stream y periódicamente: borra las películas cacheadas menos usadas hasta quedar debajo del límite, **nunca** las que tienen un stream abierto ni nada de `library/`. Comandos `get_storage_usage` y `clear_cache` (vacía `cache/` salvo los streams abiertos y `cache/img`).
  - **Investigar el espacio en disco:** al vaciar la caché con la app cerrada se liberó en `df` más del doble de lo que medía `du` (5,7 GB en `du` y unos 13,6 GB en `df`). Revisar si quedan archivos borrados pero abiertos (`ls -l /proc/<pid>/fd | grep deleted`), preasignación de librqbit, o el DHT y los archivos de sesión. Corregir o documentar.
- **frontend:**
  - Botón ♥ en la Ficha y en el hero, con actualización optimista y deshacer si falla. Página **Mi lista** con estado vacío.
  - Fila **Continuar viendo** como primera fila del Inicio, con barra de progreso y opción para quitarla. En la Ficha: **"Continuar (h:mm:ss)"** y **"Desde el principio"**.
  - Reproductor: `save_progress` cada 10 s, al pausar, al salir y al terminar. Retomar en `resumeAtS`.
  - **Ajustes** con la estructura completa del prototipo:
    - Funcionan: **Catálogo** (URLs base con su latencia en vivo vía `get_api_status`), **Reproducción** (calidad preferida, x264, reproductor externo, búfer) y **Almacenamiento** (uso de la caché vs. el límite, editar el límite y el botón **"Vaciar caché ahora"**).
    - Visibles con "Próximamente": Subtítulos (fase 5) y Torrent (fase 6).
  - `pickDefaultTorrent` respeta `preferredQuality` y `preferX264` de los ajustes.
- **Tests:**
  - Backend: migraciones en una DB temporal (desde cero y entre versiones), CRUD, regla del 92 %, `SettingsPatch` (null borra, ausente no toca), validación, reescritura de URLs al leer, LRU (respeta streams abiertos y `library/`) y `clear_cache`.
  - Frontend: ♥ optimista y su deshacer, fila Continuar viendo, temporizador de `save_progress` (fake timers), retomar, formulario de Ajustes, Almacenamiento y versión por defecto según los ajustes.
- **Cierre:** al reiniciar la app se conservan Mi lista y Continuar viendo (con portadas); la película retoma donde quedó y "Desde el principio" funciona; al pasar el 92 % sale de Continuar viendo; cambiar la calidad o las URLs en Ajustes tiene efecto sin reiniciar; la caché no pasa del límite y "Vaciar caché ahora" libera el espacio. Tag `fase-4`.

## Fase 5: Subtítulos ✅

**Resultado real:** subtítulos en español automáticos y sincronizados, retraso con G/H, `.srt` propio (diálogo o arrastrar), subtítulos y retraso también en VLC (el retraso va horneado en un `.srt` porque `--sub-delay` de VLC no aplica a `.vtt`), y con el cupo agotado el menú abre la página de OpenSubtitles. Cupo: 5 descargas/día sin sesión, 20 con cuenta gratuita; lo cacheado en disco no gasta cupo.

**Objetivo:** subtítulos en español que se cargan solos y quedan sincronizados, más la opción de cargar un `.srt` propio. Contrato: IPC v0.7.

- **backend:**
  - `subtitles.rs`: cliente REST de OpenSubtitles (`https://api.opensubtitles.com/api/v1`, cabeceras `Api-Key` y `User-Agent: YTSPlayer v<versión>`): `GET /subtitles`, `POST /download` y, si hay usuario y contraseña en Ajustes, `POST /login` con el token guardado en memoria durante la sesión (se invalida al cambiar las credenciales). Verificar las cifras reales de cupo sin cuenta y con cuenta gratuita y anotarlas.
  - Búsqueda por `imdbCode` + idioma. Orden: coincidencia con el release de YTS (calidad y fuente del infohash: `1080p`, `BluRay`/`WEB`…), luego al final los `hearingImpaired` y `aiTranslated` (salvo que no haya otros), luego más descargados.
  - Conversión SRT→VTT: BOM, latin-1/windows-1252 → UTF-8, `\r\n`, tiempos con coma, numeración rota, líneas vacías extra, etiquetas `<i>`/`<b>` (se conservan) y `{\an8}` (se quitan).
  - Caché en disco `<datos>/subs/<fileId>.vtt` (en la ubicación por defecto, como la DB): volver a cargar un subtítulo no gasta cupo. Servido en `/subs/<id>.vtt` **con cabeceras CORS** para el origen de la WebView.
  - Comandos `search_subtitles`, `load_subtitle`, `load_subtitle_file` (`.srt`/`.vtt` desde una ruta) y `get_subtitles_status`.
  - Plugin `tauri-plugin-dialog` (y su permiso de abrir archivo en `capabilities/`) para "Cargar archivo…".
  - Errores `subtitles_auth` y `subtitles_quota` (con la hora de renovación). Las credenciales y la key nunca aparecen en los logs.
- **frontend:**
  - **Carga automática:** con `autoSubtitles` y key configurada, al empezar a reproducir se busca en `subtitleLang` y se carga el primer resultado. Si no hay nada en ese idioma, aviso discreto y oferta de inglés.
  - **Capa propia de subtítulos:** el front descarga el `.vtt` con `fetch` desde el servidor local (no usa `<track>`: un track de otro origen obligaría a poner `crossorigin` en el `<video>` y a pasar el stream con `Range` por CORS) y el reproductor dibuja los `cue` activos en su propia capa (tamaño y estilo del diseño, sombra legible, sube cuando aparecen los controles). El retraso se aplica al elegir el `cue` (tiempo del video − retraso), sin depender de cómo dibuja WebKitGTK las pistas.
  - Menú de subtítulos del reproductor (como en el prototipo): desactivados, opciones (marca en las que coinciden con el release), "Cargar archivo…" y retraso ±0,1 s (atajos `G`/`H`; se recuerda por película durante la sesión).
  - Soltar un `.srt` sobre el reproductor lo carga (`onDragDropEvent` de Tauri).
  - Ajustes → Subtítulos (deja de ser "Próximamente"): API key (oculta), usuario y contraseña opcionales, botón **Probar** (`get_subtitles_status`: válida, con sesión, cupo restante), idioma y carga automática. Sin key: mensaje claro con cómo conseguirla.
  - Errores: sin key → enlace a Ajustes; cupo agotado → "se renueva a las HH:MM", y sigue disponible cargar un archivo.
- **Tests:**
  - Backend: SRT→VTT con los casos difíciles (fixtures reales), ranking, wiremock de OpenSubtitles (búsqueda, descarga, login, 401 → `subtitles_auth`, 406/429 → `subtitles_quota`) y que la caché en disco evita la segunda descarga.
  - Frontend: selección automática y caída a inglés, cue activo según el retraso, menú y atajos, estado sin key, error de cupo y carga por archivo.
- **Añadido al cierre (IPC v0.8): subtítulos en VLC.** "Abrir en VLC" le pasa a VLC el subtítulo activo (de OpenSubtitles o propio) y su retraso; si no había ninguno (caso típico del error de códec), el backend busca el mejor según los ajustes. Un fallo de subtítulos nunca impide abrir VLC: se abre sin ellos y el front muestra un aviso.
- **Añadido al cierre (IPC v0.9): cupo agotado.** Con el cupo agotado, el clic en un subtítulo no cacheado del menú abre su página de OpenSubtitles en el navegador, para descargarlo a mano y soltarlo sobre el reproductor; los cacheados se siguen cargando.
- **Cierre:** al reproducir aparecen solos los subtítulos en español y sincronizados, el retraso funciona, se puede cargar un `.srt` manual, sin key la app explica qué hacer y VLC se abre con los subtítulos. Tag `fase-5`.

## Fase 6: Descargas y Ajustes de torrent ✅

**Resultado real:** descargas con pausa/reanudación que sobreviven al reinicio, reproducción sin red desde la biblioteca, promoción de stream a descarga sin rebajar, límites de velocidad en caliente, y carpetas de descargas y caché elegibles con movimiento entre discos (progreso y cancelación). Perfil dev con `debug = "line-tables-only"`: binarios de test de ~460 MB a ~86 MB.

**Objetivo:** guardar películas para verlas sin conexión y controlar la red. (El límite de la caché, `get_storage_usage` y `clear_cache` ya se hicieron en la fase 4.) Contrato: IPC v0.10, sección Descargas.

**Decisiones:** (1) sin conexión de verdad: copia del `MovieDetail` en la DB y reproducción directa desde `library/`; (2) carpetas legibles `Título (año) [calidad]`; (3) progreso por polling de `list_downloads`, sin evento nuevo; (4) el puerto se aplica al reiniciar; (5) comprobación de espacio antes de descargar.

- **backend:**
  - Comandos `start_download`, `list_downloads`, `pause_download`, `resume_download`, `remove_download` (evict antes de borrar) y `open_download_folder`. Evento `download://changed` en cada cambio de estado.
  - Promover un stream a descarga sin volver a bajar lo descargado; desde ese momento ya no cuenta para la caché LRU.
  - Copia del `MovieDetail` al descargar; `get_movie` sin red → copia con `offline: true`.
  - `start_stream` sobre una descarga terminada sirve el archivo local (`source: "library"`), sin red ni peers.
  - Recuperar las descargas al reiniciar con su estado; seeding según `seedAfterDownload`.
  - Ajustes de torrent: límites de bajada/subida en caliente (verificar si librqbit 9 lo permite; si no, al reiniciar y avisarlo), `seedAfterDownload` en caliente, `listenPort` al reiniciar.
  - Comprobación de espacio libre antes de crear la descarga.
- **frontend:**
  - Página **Descargas** como en el prototipo: estado, progreso, velocidad, peers, ETA y acciones (pausar/reanudar, quitar con diálogo "¿borrar también los archivos?", abrir carpeta, reproducir). Estado vacío.
  - **Contador** de descargas activas en la barra de navegación (con `download://changed`).
  - **Ficha:** Descargar → "Descargando 45 %" → "Descargada ✓"; Reproducir de una descargada arranca al instante.
  - **Sin conexión:** desde Descargas se abre la Ficha y se reproduce; aviso discreto "Sin conexión" y se ocultan las filas que dependen de la red. Con `offline: true` la Ficha muestra lo que haya.
  - **Ajustes → Torrent** funcional (deja de ser "Próximamente"): límites, seeding y puerto con la nota "se aplica al reiniciar".
- **Tests:**
  - Backend: transiciones de estado, promoción de stream a descarga sin rebajar (torrent local), recuperación al reiniciar, `get_movie` offline, falta de espacio, reproducción desde `library/` y que `remove_download` libera el espacio.
  - Frontend: página Descargas con polling y eventos, contador, estados del botón de la Ficha, modo sin conexión y formulario de Torrent.
- **Añadido al cierre (IPC v0.11): carpetas elegibles.** `dataDir` se reemplaza por `downloadsDir` y `cacheDir`, elegidas con el selector de carpetas del sistema y aplicadas sin reiniciar.
  - backend: migración de `dataDir`, validación, cambio en caliente (caché vieja borrada; descargas existentes se quedan con su ruta), `move_downloads` con progreso y cancelación (rename o copiar+borrar entre discos, comprobación de espacio), estado `unavailable` con reanudación automática y respaldo de la caché a la carpeta por defecto.
  - frontend: Ajustes → Almacenamiento con las dos carpetas (selector, espacio libre y ocupado, Restablecer), oferta "Mover también las descargas existentes" con diálogo de progreso y Cancelar, y estados `unavailable`/`moving` en Descargas.
  - tests: migración, validación, mover en el mismo disco y con copia (dos tmpdirs), cancelación a mitad, falta de espacio, carpeta que desaparece y vuelve; front: selector, diálogo de progreso y estados.
- **Cierre:** descargar una película, cortar la red, reiniciar la app y reproducirla desde Descargas; las carpetas se pueden cambiar y las descargas existentes se mueven con progreso; pausar/reanudar sobrevive a un reinicio; los límites de velocidad se notan; quitar con archivos libera el espacio. Tag `fase-6`.

## Fase 7: Tráilers, pulido, robustez y E2E ✅

**Resultado real:** banner rotativo con recomendaciones e Inicio personalizado; tráiler en el modal a través de la página local `/trailer` (YouTube exige `Referer`); pantalla `no_peers`, una acción para cada `ErrorCode`, cierre ordenado (~1,4 s) y rendimiento medido (23 MB, 43 % de un núcleo con 4 descargas + 1 stream); los dos problemas de la fase 2 resueltos; E2E en dos capas en el CI (WebdriverIO sobre la app real sin internet y Playwright WebKit + axe con mocks).

**Objetivo:** una app que se siente terminada, que no deja al usuario sin salida, con un Inicio que cambia y con E2E en el CI. Contrato: IPC v0.12.

**Problemas conocidos que se arrastran (el usuario los aceptó en la fase 2, se arreglan aquí):**
- Buscar: los filtros se pierden al ir a Inicio y volver por la navegación (atrás/adelante sí los conserva). Recordar la última búsqueda (p. ej. en Zustand) al volver a `/search`.
- Teclado: en la app real las flechas solo desplazan la página y no mueven el foco entre tarjetas ni filas (los tests unitarios pasan, así que revisar el foco inicial y el manejo de eventos dentro de la WebView).

- **backend:**
  - **Recomendaciones:** `get_featured` (6 películas con su motivo, a partir del historial + `movie_suggestions` + géneros más vistos; estable en la sesión y distinto en cada arranque) y `get_home_profile` (fila "Porque viste X" y orden de géneros por afinidad). Sin red, vacíos sin fallar.
  - **Tráiler:** revisar `open_trailer_window` (ventana aparte) y si un `Referer`/origen correcto evita el error 153 de YouTube; documentar lo que funciona.
  - **Sin peers:** fase `no_peers` a los 60 s sin haber conectado nunca a un peer.
  - **`app://error`** donde todavía falte (disco lleno durante una descarga, servidor local caído…).
  - **Cierre ordenado:** al cerrar la ventana, pausar torrents, guardar el estado de las descargas, liberar el puerto y cerrar la DB, con un tiempo máximo.
  - **Rendimiento:** memoria y CPU con 3–4 descargas + un stream; ninguna tarea en segundo plano huérfana. Anotar las cifras.
  - `StorageUsage.defaultDownloadsDir`/`defaultCacheDir`.
  - **Soporte E2E:** respetar `XDG_DATA_HOME`, `YTS_PLAYER_API_BASE_URLS` y `YTS_PLAYER_NO_DHT`, y un seeder de prueba (`cargo run --example e2e_seeder -- <archivo>`: crea el `.torrent`, siembra en localhost e imprime el infohash y la ruta del `.torrent`).
- **frontend:**
  - **Banner rotativo:** las 6 de `get_featured` con su motivo ("Porque viste X", "Porque está en tu lista", "Para ti: <género>", "Tendencia"); avanza cada ~8 s con fundido y precarga de la siguiente; se pausa con hover, foco o ventana oculta; puntos, flechas y teclado; sin avance automático con `prefers-reduced-motion`; si viene vacío, el banner de siempre.
  - **Inicio personalizado:** fila "Porque viste X" y filas de géneros en el orden de `genreOrder`.
  - **Tráiler:** primero comprobar en la WebView real qué funciona. Cadena: modal (iframe con la API de YouTube para detectar errores) → `open_trailer_window` → abrir en el navegador; siempre un botón "Ver en YouTube".
  - **Errores:** una pantalla o mensaje con acción para cada `ErrorCode`, más la pantalla `no_peers` (otra versión con más seeds / seguir esperando / volver).
  - Los dos problemas de la fase 2 (filtros de Buscar y flechas).
  - **Accesibilidad:** foco visible, toda la app con teclado, `prefers-reduced-motion`, contraste AA.
  - **Pulido:** micro-interacciones según `DESIGN.md` y revisión de cada pantalla contra el prototipo.
  - "Restablecer" carpetas con `defaultDownloadsDir`/`defaultCacheDir` (sin adivinar por el sufijo).
  - `data-testid` estables en lo que usan los E2E (los pide `plan`).
- **plan:**
  - **E2E capa 1, app real:** `e2e/app/` con WebdriverIO + `tauri-driver` (+ `WebKitWebDriver`), un servidor falso de YTS que sirve fixtures con los `.torrent` del seeder local y un MP4 H.264 corto generado con ffmpeg. Flujos: Inicio → Ficha → Reproducir → progreso guardado → Mi lista → Descargar → reproducir sin red.
  - **E2E capa 2, interfaz:** `e2e/ui/` con Playwright (WebKit) contra Vite con los mocks del front: teclado y foco, banner, filtros de Buscar, pantallas de error, accesibilidad (axe) y capturas.
  - **CI:** job de Playwright en cada push; job de la app real con Xvfb, `webkit2gtk-driver`, GStreamer y el seeder.
  - `docs/QA.md`: lista de verificación manual completa, que se repasa con el usuario al final.
- **Tests:** cada bug corregido con su test. Backend: recomendaciones (mezcla de motivos, descartes, sin historial, sin red), `no_peers`, cierre ordenado, variables de entorno y seeder. Frontend: carrusel (avance, pausa, reduced-motion), cadena del tráiler, pantallas de error, foco con flechas y filtros que se conservan.
- **Cierre:** E2E de las dos capas en verde en el CI, el banner rota con recomendaciones y cambia entre arranques, cada `ErrorCode` tiene su pantalla o mensaje con acción, el tráiler se ve de alguna forma, los dos problemas de la fase 2 están resueltos y `QA.md` está repasado. Tag `fase-7`.

## Fase 8: Empaquetado y release v1.0 ✅

**Resultado real:** `.deb` y `.rpm` de 9 MB y AppImage de 148 MB, compilados en Ubuntu 22.04 por `release.yml`. Smoke test en el CI: el `.deb` se instala en un Ubuntu 24.04 limpio y pasa los E2E sobre la app instalada; el AppImage arranca (en un runner sin escritorio necesita las librerías gráficas del sistema: EGL y GLES). Ensayo `v1.0.0-rc.1` probado por el usuario. Licencia MIT. De paso se encontró y rodeó un bug de librqbit (pieza perdida al pausar). Siguiente: Windows (v1.1).

**Objetivo:** que la app se instale como cualquier otra y publicar la v1.0.0 en GitHub Releases ya verificada. Contrato: IPC v0.13 (sección Aplicación).

**Decisiones:** (1) solo Linux en la v1.0: `.deb`, `.rpm` y AppImage (Windows y macOS al backlog: en Windows el iframe del tráiler se bloquea por contenido mixto); (2) se compila en Ubuntu 22.04 (glibc más viejo = más distros); (3) `.deb`/`.rpm` dependen de GStreamer (libav, good, bad) y recomiendan VLC; el AppImage incluye GStreamer (`bundleMediaFramework`); (4) actualizaciones solo con aviso (GitHub Releases), sin auto-update firmado; (5) la release se arma como borrador y solo se publica tras el smoke test en el CI y la prueba del usuario; (6) logs a archivo con rotación.

- **plan:**
  - Workflow `release.yml`: con un tag `v*` compila en `ubuntu-22.04` (`tauri-action`), comprueba que la versión coincide con el tag, crea un **borrador** de release con `.deb`, `.rpm` y AppImage, y corre el smoke test: instala el `.deb` en un contenedor Ubuntu 24.04 limpio y el AppImage, y pasa los E2E de la app real (`e2e/app`, sin internet) sobre el binario instalado.
  - `scripts/bump-version.sh <versión>`: sincroniza `package.json`, `src-tauri/Cargo.toml` y `src-tauri/tauri.conf.json`.
  - `CHANGELOG.md` (fases 1–8) y README completo: qué es, capturas (sacadas de la app real), instalación por formato, dependencias, API key de OpenSubtitles, compilar desde el código, aviso legal.
- **backend:**
  - Metadatos del bundle: versión 1.0.0, categoría `AudioVideo`, descripciones corta y larga, dependencias de `.deb` y `.rpm`, `bundleMediaFramework` en AppImage, archivo `.desktop` correcto.
  - Release limpio: sin logs de depuración ni rutas de desarrollo; las variables de E2E siguen funcionando pero no aparecen en la UI ni en los logs salvo un `info` al arrancar.
  - Logs a archivo con rotación (`$XDG_STATE_HOME/yts-player/logs`), `get_app_info`, `check_for_update` y `open_logs_folder`.
  - Iconos en todas las resoluciones con `npx tauri icon` a partir del SVG de frontend.
  - Que `e2e/app` pueda usar un binario instalado: respetar `E2E_APP_BINARY` en la config (lo hace `plan`), y que el binario no dependa de nada del repo.
- **frontend:**
  - **Icono de la app**: diseño propio en SVG (`design/icon/app-icon.svg`, 1024×1024, legible a 32 px), coherente con `DESIGN.md`.
  - **Acerca de** (en Ajustes): versión, repo, aviso legal, créditos (YTS, OpenSubtitles, librqbit, Tauri), "Abrir carpeta de registros".
  - Aviso discreto de **nueva versión disponible** (una vez por sesión, se puede descartar) con enlace a la release.
  - Revisión final de todos los textos en español.
- **Tests:** smoke test de instalación en el CI (arriba); backend: comparación semver y parseo de la respuesta de GitHub (wiremock), rotación de logs; frontend: Acerca de y aviso de actualización.
- **Cierre:** release `v1.0.0` publicada con `.deb`, `.rpm` y AppImage, el smoke test en verde y el usuario la instala en su máquina (la app abre, el catálogo carga y se reproduce una película). Tag `v1.0.0`.

---

## Más allá de la v1.0 (backlog, sin fecha)
- Builds para Windows y macOS (WebView2 y WKWebView tienen su propio soporte de códecs, así que hay que revisar HEVC).
- Integrar libmpv para reproducir HEVC/x265 dentro de la app sin VLC.
- Auto-actualizaciones (`tauri-plugin-updater`).
- Varios idiomas de subtítulos al mismo tiempo y subtítulos generados en local.
- Recomendaciones según el historial.
- Perfiles de usuario.
