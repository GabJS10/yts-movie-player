# AGENTS.md

Guía para agentes de IA (y personas) que trabajen en este repositorio.

## Proyecto
**YTS Player**: app de escritorio estilo Netflix para explorar el catálogo de YTS y reproducir películas en streaming desde torrent (con opción de descargarlas), subtítulos en español automáticos, trailers, "Mi lista" y "Continuar viendo".

- Plan completo: [`docs/PLAN.md`](docs/PLAN.md)
- Referencia de la API de YTS: [`docs/YTS-API.md`](docs/YTS-API.md)
- Sistema de diseño (tokens, tipografía, componentes): [`DESIGN.md`](DESIGN.md); contexto de producto: [`PRODUCT.md`](PRODUCT.md)
- Prototipo navegable de todas las pantallas: `design/prototype/index.html` (servir con `python3 -m http.server` desde esa carpeta)

**Estado actual:** planificación y diseño (prototipo) terminados; todavía no hay código de la app. Siguiente paso: Fase 1 (base del proyecto) y Fase 2 (catálogo) de `docs/PLAN.md`.

## Forma de trabajo (multi-agente)
El proyecto se desarrolla con **3 agentes en paralelo**, cada uno en su propia tab del workspace `yts-movie-player` de [Herdr](https://herdr.dev). Hoy los tres son Claude Code, pero el flujo no depende del harness (puede entrar Codex u otro): **este `AGENTS.md` es la fuente de verdad**, y `CLAUDE.md` solo lo importa.

| Agente (nombre en Herdr) | Tab | Responsabilidad | Es dueño de |
|---|---|---|---|
| `plan` | `plan` | Orquestación, planificación general, decisiones de arquitectura, contrato entre front y back, tareas comunes (CI/CD, empaquetado, releases, tooling de la raíz) | `docs/`, `AGENTS.md`, `CLAUDE.md`, `.github/`, configs de la raíz no específicas del front |
| `frontend` | `frontend` | UI en React, diseño, prototipo, estado del cliente y wrappers de `invoke()` | `src/`, `design/`, `DESIGN.md`, `PRODUCT.md`, `index.html`, `vite.config.*`, `tailwind.config.*`, `tsconfig*.json`, dependencias npm |
| `backend` | `backend` | Core en Rust: API YTS, torrent, servidor de streaming, subtítulos, DB y comandos Tauri | `src-tauri/` (incluye `Cargo.toml` y `tauri.conf.json`) |

### Reglas de coordinación
- **Cada agente edita solo sus archivos.** Si necesitas un cambio en otra área, pídeselo a su dueño (o a `plan`) en lugar de hacerlo tú.
- **Contrato IPC en `docs/IPC.md`**: los comandos Tauri (nombre, parámetros, tipo de retorno y errores) y los eventos (`torrent://stats`, etc.) con sus payloads. `backend` lo propone o actualiza al crear o cambiar un comando, y `frontend` lo implementa en `src/api/tauri.ts`. Un cambio que rompa el contrato se coordina antes a través de `plan`.
- **`plan` reparte el trabajo** siguiendo las fases de `docs/PLAN.md`, y cada agente avisa a `plan` cuando termina una tarea o queda bloqueado.
- Comunicación entre agentes con la CLI de Herdr, usando el nombre del agente (no el ID del pane):
  ```bash
  herdr agent list
  herdr agent prompt backend "..." --wait --timeout 600000
  herdr agent read frontend --source recent-unwrapped --lines 120
  ```
- No mandes un prompt a un agente que está `working` o `blocked` sin revisar antes su estado (`herdr agent get <nombre>`).
- Decisiones de arquitectura nuevas: las registra `plan` en `docs/PLAN.md` (o en `docs/decisions/`).
- No cerrar ni mover tabs/panes de otros agentes.

## Stack
- **Escritorio:** Tauri 2
- **Frontend:** React + TypeScript + Vite, Tailwind, TanStack Query (datos remotos), Zustand (estado de UI), React Router
- **Backend (Rust):** `reqwest` + `serde` (API YTS), `librqbit` (motor torrent y streaming), `axum` o el servidor HTTP de librqbit (stream local con `Range`), `rusqlite` (persistencia), `tokio`
- **Subtítulos:** API REST de OpenSubtitles (requiere API key del usuario en Ajustes)
- **Plataforma objetivo:** Linux primero (WebKitGTK); Windows y macOS después

## Estructura (prevista)
```
src/                 # Frontend React
  api/tauri.ts       # wrappers tipados de invoke(); único punto de contacto con Rust
  pages/             # Home, Search, MovieDetail, Player, MyList, Downloads, Settings
  components/        # MovieRow, MovieCard, HeroBanner, Filters, TrailerModal, PlayerControls
  store/
src-tauri/src/
  yts.rs             # cliente API + modelos
  torrent.rs         # sesión librqbit
  stream.rs          # servidor HTTP local
  subtitles.rs       # OpenSubtitles + SRT→VTT
  db.rs              # SQLite + migraciones
  commands.rs        # #[tauri::command]
docs/                # documentación del proyecto
```

## Comandos
```bash
npm install                 # dependencias del frontend
npm run tauri dev           # app en modo desarrollo
npm run tauri build         # empaquetado (.deb / AppImage)
cd src-tauri && cargo test  # tests de Rust
cd src-tauri && cargo clippy -- -D warnings
npm run lint && npm run typecheck
```
Dependencias del sistema (Ubuntu): `libwebkit2gtk-4.1-dev build-essential libssl-dev librsvg2-dev libayatana-appindicator3-dev`. Para reproducir H.264: `gstreamer1.0-libav gstreamer1.0-plugins-bad`.

## Reglas y convenciones
- **URL de la API YTS nunca fija en el código.** Lista de URLs base configurable con failover (`movies-api.accel.li` y luego `yts.gg`) y override desde Ajustes. Ver `docs/YTS-API.md`.
- **Modelos serde tolerantes:** la API devuelve tipos inconsistentes (`is_repack` como `"0"`/`""`, ausencia de `movies` si no hay resultados). Usar `#[serde(default)]` y deserializadores flexibles; usar `size_bytes`, no `size`.
- **Toda la red, el torrent y el disco van en Rust.** El frontend solo llama a comandos Tauri (vía `src/api/tauri.ts`) y escucha eventos (`torrent://stats`, etc.). Nada de `fetch` directo a YTS desde React.
- **El streaming se sirve por HTTP en `127.0.0.1`** con soporte de `Range` (respuesta `206`); el `<video>` apunta a esa URL.
- **Preferir torrents x264.** x265/HEVC (habitual en 2160p) probablemente no se reproduce en WebKitGTK; ofrecer el botón "Abrir en VLC".
- Datos de usuario en `~/.local/share/yts-player/` (`cache/` para el streaming con límite LRU y `library/` para las descargas guardadas).
- Errores en Rust con `thiserror`; los comandos devuelven `Result<T, String>` serializable. Nada de `unwrap()` en caminos de producción.
- TypeScript en modo `strict`. Componentes funcionales y hooks.
- La UI de la app está en **español**. Identificadores y comentarios de código en inglés.
- Tests: fixtures JSON reales de la API para el parseo, más tests de SRT→VTT, del armado de magnets y de las migraciones de la DB.
- Mantener `docs/PLAN.md` actualizado si cambian las decisiones de arquitectura.
