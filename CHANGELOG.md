# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/); versionado [semántico](https://semver.org/lang/es/).

## [1.0.0] - 2026-10-04

Primera versión pública. Linux: `.deb`, `.rpm` y AppImage.

### Catálogo
- Inicio estilo Netflix con banner rotativo de recomendaciones según tu historial ("Porque viste X", "Porque está en tu lista", tus géneros) y filas personalizadas.
- Búsqueda con filtros de género, calidad, valoración y orden, que se conservan al volver.
- Ficha con sinopsis, reparto, capturas, versiones (calidad, códec, seeds) y Similares.
- La URL de la API de YTS es configurable, con failover automático entre dominios.

### Reproducción
- Streaming desde el torrent con búfer de ~8 MB: la película empieza en segundos y se puede adelantar a cualquier punto.
- Controles propios con atajos (espacio, ←/→, F, M, G/H) y barra con visto / en búfer / descargado.
- Versiones x265/HEVC: "Abrir en VLC" (o mpv) con los subtítulos y su retraso.
- Pantalla "sin peers" a los 60 s con alternativas.

### Subtítulos
- Subtítulos en español automáticos desde OpenSubtitles (API key gratuita), ordenados por coincidencia con el release.
- Retraso ajustable, `.srt` propio por diálogo o arrastrándolo, caché en disco que no gasta cupo y página de OpenSubtitles cuando se agota el cupo.

### Mi lista, Continuar viendo y descargas
- Mi lista y Continuar viendo (retoma en el segundo exacto; a partir del 92 % se da por vista).
- Descargas para ver sin conexión, con pausa/reanudación que sobrevive al reinicio y paso de streaming a descarga sin volver a bajar.
- Carpetas de descargas y de caché elegibles, con movimiento entre discos.
- Límite de caché LRU, límites de velocidad en caliente y opción de seguir compartiendo.

### Calidad
- Una pantalla o mensaje con acción para cada error; modo sin conexión.
- Tráiler en la app (con alternativas si YouTube no lo permite).
- Navegación completa con teclado, foco visible, `prefers-reduced-motion` y contraste AA.
- Logs a archivo con rotación, "Acerca de" y aviso de nuevas versiones.
- Tests: unitarios e integración en Rust (torrent local sin internet), Vitest en el front, E2E con WebdriverIO sobre la app real y Playwright + axe sobre la interfaz.

[1.0.0]: https://github.com/GabJS10/yts-movie-player/releases/tag/v1.0.0
