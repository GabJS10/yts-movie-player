<p align="center">
  <img src="design/icon/app-icon.svg" width="112" alt="" />
</p>

<h1 align="center">YTS Player</h1>

<p align="center">
  Una app de escritorio estilo Netflix para el catálogo de YTS: explora, mira en streaming desde el torrent y descarga para ver sin conexión.
</p>

<p align="center">
  <a href="https://github.com/GabJS10/yts-movie-player/releases/latest">Descargar</a> ·
  <a href="CHANGELOG.md">Novedades</a> ·
  <a href="docs/PLAN.md">Arquitectura</a>
</p>

<!-- SCREENSHOTS -->

## Qué hace
- **Inicio que cambia contigo:** banner rotativo con recomendaciones según lo que viste, tu lista y tus géneros, más filas de tendencias, recientes y mejor valoradas.
- **Streaming inmediato:** la película empieza en segundos mientras se descarga el torrent, y puedes adelantar a cualquier punto.
- **Subtítulos en español automáticos** desde OpenSubtitles, con retraso ajustable o tu propio `.srt`.
- **Mi lista y Continuar viendo**, que retoma en el segundo exacto.
- **Descargas** para ver sin conexión, con pausa, reanudación y la carpeta que elijas.
- **Tráilers**, búsqueda con filtros, navegación completa con teclado y modo sin conexión.
- Versiones x265/HEVC que la app no puede decodificar se abren en **VLC** con sus subtítulos.

## Instalación (Linux)
Descarga el paquete de tu distribución desde la [última release](https://github.com/GabJS10/yts-movie-player/releases/latest).

| Distribución | Paquete | Instalar |
|---|---|---|
| Ubuntu, Debian, Linux Mint, Pop!_OS | `.deb` | `sudo apt install ./YTS.Player_1.0.0_amd64.deb` |
| Fedora, openSUSE | `.rpm` | `sudo dnf install ./YTS.Player-1.0.0-1.x86_64.rpm` |
| Cualquier otra | AppImage | `chmod +x YTS.Player_1.0.0_amd64.AppImage && ./YTS.Player_1.0.0_amd64.AppImage` |

El `.deb` y el `.rpm` instalan solos lo necesario para reproducir video (GStreamer con H.264/AAC). El AppImage ya lo trae incluido. Recomendado: **VLC**, para las versiones 2160p x265.

La app avisa cuando hay una versión nueva.

## Subtítulos: tu API key de OpenSubtitles
1. Crea una cuenta gratuita en [opensubtitles.com](https://www.opensubtitles.com).
2. En tu perfil, entra a **API consumers** y crea uno: copia la **API key**.
3. En la app: **Ajustes → Subtítulos**, pega la key y pulsa **Probar**.

Sin iniciar sesión, OpenSubtitles permite unas 5 descargas al día; con usuario y contraseña (opcionales, en el mismo lugar), unas 20. Un subtítulo ya descargado se guarda y no vuelve a gastar cupo. La key y las credenciales solo se guardan en tu equipo.

## Dónde guarda las cosas
| Qué | Dónde |
|---|---|
| Ajustes, Mi lista, progreso, subtítulos | `~/.local/share/yts-player/` |
| Caché de streaming (con límite, 10 GB por defecto) | `~/.local/share/yts-player/cache/` o la carpeta que elijas |
| Descargas | `~/.local/share/yts-player/library/` o la carpeta que elijas |
| Logs | `~/.local/state/yts-player/logs/` (Ajustes → Acerca de → Abrir carpeta de registros) |

## Compilar desde el código
Requisitos: Node 24, Rust estable y, en Ubuntu/Mint:
```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev librsvg2-dev libayatana-appindicator3-dev \
  gstreamer1.0-libav gstreamer1.0-plugins-good gstreamer1.0-plugins-bad
```
```bash
npm install
npm run tauri dev      # desarrollo
npm run tauri build    # paquetes en src-tauri/target/release/bundle/
```
Tests: `npm test`, `cd src-tauri && cargo test`, y los E2E en [`e2e/`](e2e/) (`npm run ui` con Playwright, `npm run app` con WebdriverIO sobre la app real). El proyecto se desarrolló por fases con varios agentes de IA coordinados; ver [`AGENTS.md`](AGENTS.md) y [`docs/ROADMAP.md`](docs/ROADMAP.md).

**Stack:** Tauri 2, React 19 + TypeScript, TanStack Router/Query, Tailwind; en Rust, librqbit (torrent), axum (servidor local con `Range`), rusqlite y reqwest.

## Aviso legal
YTS Player es solo un **cliente**: no aloja, sube ni distribuye contenido. Muestra el catálogo público de la API de YTS y usa BitTorrent para descargar lo que tú eliges; mientras descargas, también compartes partes del archivo con otros usuarios.

Gran parte de ese catálogo está protegido por derechos de autor, y descargarlo o compartirlo puede ser ilegal en tu país. **El uso de la app es responsabilidad de quien la usa.** Este proyecto no tiene relación con YTS ni con OpenSubtitles.
