# QA manual

Lista de verificación sobre la **app real** (`npm run tauri dev` o el `.deb`/AppImage) con torrents reales de YTS. Cubre lo que los E2E no pueden: la red real, los códecs, el rendimiento, el escritorio y los servicios externos. Se repasa completa antes de cada release (cierre de la fase 7 y de la fase 8) y por partes cuando se toca un área.

Cómo marcar: `[x]` bien, `[!]` falla (anotar el detalle debajo), `[-]` no aplica.

**Automatizado (no hace falta repetirlo a mano):** navegación, Mi lista, progreso, descargas, pantallas de error y accesibilidad (axe) en `e2e/ui/`; el flujo completo con torrent local y sin red en `e2e/app/`. Ver `e2e/` y el CI.

## 0. Preparación
- [ ] Partir de datos limpios: cerrar la app y mover `~/.local/share/yts-player/` a un respaldo (o usar `XDG_DATA_HOME=/tmp/qa`).
- [ ] Anotar: versión/commit, distro, versión de WebKitGTK (`dpkg -l libwebkit2gtk-4.1-0`), si hay VLC y mpv.

## 1. Primer arranque e Inicio
- [ ] La ventana abre en 1440×900 con fondo oscuro, sin parpadeo blanco.
- [ ] Sin historial: el banner rota entre ~6 películas con el motivo "Tendencia"; las filas cargan con portadas.
- [ ] El banner avanza solo (~8 s), se pausa con el mouse encima y con el foco; puntos y flechas funcionan; con "reducir movimiento" del sistema no avanza solo.
- [ ] Scroll horizontal fluido en las filas; las portadas no saltan al cargar.
- [ ] Teclado: Tab llega a todo, el foco se ve, las flechas mueven el foco entre tarjetas y filas, Enter abre la Ficha.

## 2. Buscar
- [ ] Buscar "matrix" da resultados con debounce (sin una búsqueda por tecla).
- [ ] Filtros de género, calidad, rating y orden se combinan bien.
- [ ] Ir a Inicio y volver a Buscar **conserva** la búsqueda y los filtros.
- [ ] Una búsqueda sin resultados muestra un estado vacío claro.

## 3. Ficha
- [ ] Fondo nítido (captura), sinopsis, rating, duración, reparto con fotos, Similares.
- [ ] La versión por defecto respeta la calidad preferida y x264 de Ajustes; las versiones con pocos seeds avisan.
- [ ] **Tráiler:** se reproduce en el modal. Si falla, se abre la ventana aparte; si falla también, el navegador. "Ver en YouTube" siempre está.
- [ ] ♥ Mi lista cambia al instante y persiste tras reiniciar.

## 4. Streaming
- [ ] 1080p x264 popular: empieza en < 30 s; la pantalla de búfer muestra fase, peers, velocidad y mapa de piezas.
- [ ] Adelantar a cualquier punto (barra y ←/→) sigue reproduciendo; la barra muestra visto / en búfer / descargado.
- [ ] Espacio, F, M y G/H funcionan; el icono play/pausa no se queda pegado tras adelantar.
- [ ] 2160p x265: pantalla de códec con "Abrir en VLC"; VLC abre con subtítulos y el retraso aplicado.
- [ ] Una versión sin seeds: a los 60 s aparece la pantalla "sin peers" con otra versión / seguir esperando / volver.
- [ ] Salir a mitad y volver: "Continuar (h:mm:ss)" retoma en el segundo exacto; "Desde el principio" empieza en 0.
- [ ] Pasar el 92 %: sale de Continuar viendo.

## 5. Subtítulos (con la API key en Ajustes)
- [ ] Al reproducir aparecen solos los subtítulos en español y sincronizados.
- [ ] El menú marca las versiones que coinciden con el release; cambiar de subtítulo y desactivarlos funciona.
- [ ] Cargar un `.srt` en latin-1 (tildes y ñ) por diálogo y arrastrándolo.
- [ ] Sin key: el aviso explica cómo conseguirla. Cupo agotado: el menú abre la página de OpenSubtitles.

## 6. Descargas y almacenamiento
- [ ] Descargar desde la Ficha: Descargar → "Descargando X %" → "Descargada ✓"; contador en la barra.
- [ ] Pausar, cerrar la app, abrir: sigue en pausa con su progreso; reanudar.
- [ ] Empezar a ver y pulsar Descargar: al salir del reproductor se mueve a la biblioteca sin volver a bajar.
- [ ] **Sin red** (desconectar wifi/cable) y reiniciar: aviso "Sin conexión", la Ficha descargada abre y reproduce al instante.
- [ ] Límite de bajada (p. ej. 500 KB/s): la velocidad baja al momento.
- [ ] Quitar con archivos: `df -h` muestra el espacio liberado.
- [ ] Cambiar la carpeta de descargas a otra partición y "Mover": progreso, Cancelar a mitad deja la película en su sitio; mover todo.
- [ ] Desmontar un USB con descargas: "Carpeta no disponible"; montarlo: se reanudan solas.
- [ ] "Vaciar caché" libera el espacio y la caché respeta el límite.

## 7. Errores y robustez
- [ ] Ajustes → Catálogo con una URL base inválida primero: el failover usa la siguiente sin que se note.
- [ ] Todas las URLs inválidas: pantalla "API no disponible" con Reintentar y enlace a Ajustes.
- [ ] Disco casi lleno durante una descarga: aviso claro, la descarga queda en error y se puede reanudar al liberar espacio.
- [ ] Sin VLC instalado: "Abrir en VLC" explica que falta.
- [ ] Ningún error deja la pantalla en blanco ni sin una acción para seguir.

## 8. Cierre y rendimiento
- [ ] Cerrar la ventana con un stream y 2 descargas activas: cierra en < 3 s; al reabrir, las descargas siguen y no hay procesos `yts-player` colgados (`ps`).
- [ ] Con 3–4 descargas y un stream: CPU y memoria razonables (anotar las cifras) y la interfaz sigue fluida.
- [ ] Una hora reproduciendo: sin fugas de memoria visibles ni cortes.

## 9. Accesibilidad y pulido
- [ ] Toda la app usable solo con teclado; foco visible en todo.
- [ ] Contraste legible en textos secundarios; nada depende solo del color.
- [ ] Las pantallas coinciden con el prototipo (`design/prototype/`); textos en español sin erratas.

## Registro
| Fecha | Versión / commit | Quién | Resultado | Notas |
|---|---|---|---|---|
