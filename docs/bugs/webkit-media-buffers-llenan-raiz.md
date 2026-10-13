# Bug: el reproductor llena la partición raíz con buffers de video en `/var/tmp`

- **Versión:** v1.2.0
- **Plataforma:** Linux (WebKitGTK 4.1)
- **Detectado:** 2026-10-11
- **Estado:** resuelto (2026-10-12), con QA manual hecha

## Síntoma

Linux avisó de que la partición raíz se estaba quedando sin espacio (solo quedaban 642M libres). Esto ocurrió con `yts-player` abierto durante unos 80 minutos.

## Evidencia (salida de `sudo lsof +L1`)

```
WebKitWeb 2565 gabriel 37u  REG 259,6 8188101171 0 556493 /var/tmp/WebKit-Media-KM2LW3 (deleted)
WebKitWeb 2565 gabriel 79u  REG 259,6 8188101171 0 556525 /var/tmp/WebKit-Media-AMPLW3 (deleted)
WebKitWeb 2565 gabriel 150u REG 259,6 8188101171 0 565738 /var/tmp/WebKit-Media-B6I6W3 (deleted)
WebKitWeb 2565 gabriel 160u REG 259,6 8188101171 0 533751 /var/tmp/WebKit-Media-XNGYW3 (deleted)
WebKitWeb 2565 gabriel 88u  REG 259,6 1608749288 0 565726 /var/tmp/WebKit-Media-7TKZW3 (deleted)
```

- El proceso `WebKitWebProcess` (PID 2565) es hijo de `/usr/bin/yts-player` (PID 2465).
- Había **5 archivos que sumaban unos 34G**. Ya estaban borrados (`unlink`), pero el proceso los seguía teniendo abiertos, así que el espacio no se liberaba.
- Cuatro de ellos miden **exactamente lo mismo, 8 188 101 171 bytes**. Lo más probable es que sea la misma película cargada 4 veces y que cada carga dejara su propio buffer abierto.
- Al cerrar la app se liberaron los 34G de golpe.

## Causa (confirmada en el código de WebKitGTK 2.52.6)

1. **WebKit activa la descarga a disco (*on-disk buffering*) de GStreamer** cuando se cumplen todas estas condiciones:
   - La fuente no es en vivo.
   - `preload` es Auto.
   - La URL no es `blob:`.
   - El medio **no está en bucle**: WebKit comprueba `player->isLooping()` y, si lo está, escribe "Media is looping. Disabling deadlock-prone on-disk buffering" (issue #3129 de GStreamer).

   Esta condición se evalúa en `commitLoad` y en cada `play()`. `play()` fuerza siempre `preload=Auto`, y con `autoplay` WebKit ni siquiera le pasa al reproductor el `preload` del elemento. En ese modo, WebKit copia **el archivo entero** a `/var/tmp/WebKit-Media-XXXXXX` y lo borra al crearlo, pero lo mantiene abierto.
2. **La ruta `/var/tmp` está fija en el código de GTK.** No usa `g_get_tmp_dir()`, así que `TMPDIR` no la cambia. Las variables `WPE_SHELL_DISABLE_MEDIA_DISK_CACHE` y `WPE_SHELL_MEDIA_DISK_CACHE_PATH` solo existen en WPE.
3. **Quitar el `<video>` del DOM solo lo pausa.** Eso es lo que hace `removedFromAncestor` → `pauseAfterDetachedTask`. El reproductor, su pipeline y su archivo siguen vivos hasta que el recolector de basura destruye el elemento, y cada entrada al reproductor deja otra copia.
4. **Solo afecta a Linux.** WebView2, en Windows, no usa este mecanismo.

## Cómo reproducirlo

1. Abrir `yts-player` en Linux.
2. Reproducir una película grande (las de este caso pesaban ~8G) y dejar que cargue un rato.
3. Salir del reproductor y volver a entrar a la misma película. Repetirlo 2 o 3 veces.
4. Ejecutar `sudo lsof +L1 | grep WebKit-Media` y `df -h /`: se ve un archivo `(deleted)` nuevo por cada entrada y el espacio libre no vuelve hasta cerrar la app.

## Solución

Solo cambios de frontend; no hay cambios en el backend.

1. **(A) Liberar el pipeline.** `releaseMedia()` en `src/lib/player.ts` llama a `pause()`, a `removeAttribute("src")` y a `load()`. `load()` pasa por `prepareForLoad` → `createMediaPlayer()`, que destruye el reproductor anterior junto con su archivo. `Player.tsx` lo llama desde la limpieza del ref del `<video>`, que cubre cualquier desmontaje, y al entrar en `codec-error`, porque esa pantalla puede quedarse abierta mientras VLC reproduce.
2. **(B) Guarda de `loop`.** El `<video>` lleva `loop`, así que WebKit no activa la descarga a disco. `shouldLoop()` en `src/lib/player.ts` quita el `loop` en los últimos 5 s (`LOOP_GUARD_END_S`), y también al saltar al final con `seekTo`, para que el `ended` real siga guardando la película como vista. En `ended` se vuelve a poner el `loop`.

**Solo en WebKitGTK** (`isWebKitGtk()`, por el user agent): WebView2 no tiene este buffer. En la v1.2.1 también se aplicaba en Windows, y el E2E de Windows empezó a fallar con «la descarga no terminó». Se atribuyó a este arreglo y la v1.2.2 lo limitó a Linux, pero esa no era la causa. El fallo era intermitente y venía de una carrera al salir del reproductor: `AppShell` lo volvía a montar un instante, y el `stop_stream` de ese segundo montaje adelantaba a su `start_stream`. El stream quedaba activo y la descarga no salía de `cache/`. Se corrigió en el commit `809353b`.

**Descartadas:**

- `TMPDIR`: GTK ignora la variable (ruta fija `/var/tmp`). Además, en esta máquina `/var/tmp` y `~/.local/share` están en la misma partición.
- Responder sin `Content-Length`: si la fuente admite búsquedas, WebKit fuerza `m_isLiveStream = false`. Quitar los `Range` rompería el adelantar y retroceder.
- `preload="metadata"`: `play()` fuerza `preload=Auto`.

**Riesgo:** (B) se apoya en un detalle interno de WebKit. Si una versión futura deja de comprobar `loop`, (A) sigue limitando el daño a una copia por sesión que se libera al salir, y el E2E lo detecta.

## Verificación

- **Tests unitarios** en `src/lib/player.test.ts` y `src/components/player/Player.test.tsx`: liberación al desmontar y en `codec-error`, y la guarda de `loop`.
- **E2E en Linux**, `e2e/app/specs/flow.e2e.ts`: busca en `/proc` los fd de los `WebKitWebProcess` de la app que apunten a `/var/tmp/WebKit-Media-*`. Comprueba que no haya ninguno durante la reproducción ni después de salir.
- **En la app real:**
  - Con `GST_DEBUG='webkit*:5'` debe aparecer "Media is looping. Disabling deadlock-prone on-disk buffering" y nunca "Enabling on-disk buffering".
  - `for p in $(pgrep -x WebKitWebProces); do ls -l /proc/$p/fd | grep WebKit-Media; done` debe salir vacío.
- **Resultado del E2E (2026-10-12, WebKitGTK 2.52.6):** pasa. En el log de GStreamer, los dos reproductores (stream del torrent y biblioteca) registran "Media is looping. Disabling deadlock-prone on-disk buffering" y nunca "Enabling on-disk buffering". Al salir del reproductor aparece `tearDown … Disposing player` en ese mismo momento, sin esperar al recolector de basura.
- **QA manual (2026-10-12):** con una compilación que incluye el arreglo, después de reproducir y salir del reproductor, el comando de `/proc` sale vacío. Ojo: un `yts-player` 1.2.0 abierto (el paquete instalado, sin el arreglo) sigue dejando su copia en `/var/tmp`.
- **Efecto visible esperado:** la capa "buffered" de la barra de progreso ahora es corta. Lo descargado del torrent lo sigue mostrando `availableRanges`.
