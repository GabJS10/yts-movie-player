# Web del proyecto (brief para el agente `web`)

Web pública de YTS Player: landing + documentación, estática con **Astro**, desplegada en **Netlify** (subdominio `*.netlify.app`).

- **Repo:** aparte, `/home/gabriel/datos/projects/yts-movie-player-web` → GitHub `GabJS10/yts-movie-player-web`.
- **Agente:** `web` (tab `web` de Herdr). Es dueño de ese repo. Este repo (`/home/gabriel/datos/projects/yts-movie-player`, el de la app) **solo se lee**, nunca se edita desde `web`; si algo de aquí debe cambiar, se pide a `plan`.
- **Orquesta:** `plan`.

## Público y objetivo
La persona de [`PRODUCT.md`](../PRODUCT.md): alguien en Linux o Windows que hoy baja películas a mano de yts.gg y quiere explorar el catálogo y darle a play. La web tiene que convencerle en un vistazo y llevarle a **descargar el instalador correcto para su sistema**. Todo el copy en **español**.

## Mapa del sitio
| Ruta | Contenido | Fuente |
|---|---|---|
| `/` | Landing: hero, features, capturas, descargas por SO, FAQ corta, aviso legal breve, footer | README "Qué hace", `docs/screenshots/` |
| `/descargar` | Instaladores por plataforma con comandos y avisos (SmartScreen, firewall, GStreamer, VLC) | README "Instalación (Windows/Linux)" |
| `/docs/subtitulos` | Cómo conseguir y poner la API key de OpenSubtitles, cupos | README "Subtítulos" |
| `/docs/datos` | Dónde guarda ajustes, caché, descargas y logs | README "Dónde guarda las cosas" |
| `/docs/compilar` | Compilar desde el código, stack, tests | README "Compilar desde el código" |
| `/novedades` | Changelog renderizado | `CHANGELOG.md` |
| `/legal` | Aviso legal completo y licencia | README "Aviso legal" y "Licencia" |

Footer en todas: GitHub (`https://github.com/GabJS10/yts-movie-player`), licencia MIT, Novedades, Legal.

## Dirección visual
Decidida con el usuario. **Primer paso: prototipo de la landing con `impeccable`** a partir de esto; no se construye en Astro hasta que el usuario lo apruebe.

- **Misma identidad que la app.** Tokens de [`DESIGN.md`](../DESIGN.md) tal cual:
  - Fondos: ground `#171717`, surface `#1d1d1d`, raised `#2f2f2f`; líneas `#333333` / `#4a4a4a`.
  - Texto: `#ffffff`, secundario `#c9c9c9`, muted `#919191`, faint `#6b6b6b`.
  - Verde: `#6ac045`, hover `#75c74e`, texto sobre verde `#0b1606`, wash `rgba(106,192,69,.12)`. Reservado para la acción principal (Descargar) y acentos puntuales.
  - Tipografía: **Archivo** variable. Titulares condensados (`font-variation-settings: 'wdth' 72–75`, peso 850, interlineado ~0.9); cuerpo 15–16.5 px, interlineado 1.5–1.6; labels 11.5 px peso 600. Las fuentes están en `design/prototype/fonts/`.
  - Solo tema oscuro.
- **Hero:** la captura `inicio.png` grande dentro de un marco de ventana de escritorio, con titular condensado y botón **Descargar** que detecta el SO (Windows → `.exe`, Linux → `/descargar` o `.deb` con selector), con "Otras plataformas" al lado. Debajo del botón: versión actual y plataformas.
- **Movimiento vistoso:** entradas marcadas al hacer scroll, capturas con desplazamiento o paralaje, hover expresivo en las tarjetas de features. Todo se apaga con `prefers-reduced-motion` y nada bloquea el primer render: CSS + `IntersectionObserver`; una librería pesada solo con justificación.
- **Tono comercial:** primero el gancho y los beneficios (de abrir la app a ver la película en segundos, subtítulos en español sin hacer nada, retoma justo donde lo dejaste, descarga para ver sin conexión). Lo técnico (seeds, búfer, x265/VLC, rutas) va a las docs y la FAQ.
- **Referencias:** `design/prototype/index.html` (todas las pantallas de la app; servir con `python3 -m http.server` desde esa carpeta), `DESIGN.md` (botones, foco, componentes), `design/icon/app-icon.svg` y `design/icon/preview.png`.

## Límites del copy (no negociables)
- La app es **solo un cliente**: no aloja ni distribuye contenido. No prometer "películas gratis" ni nada parecido.
- No presentar la marca YTS como propia ni sugerir relación con YTS u OpenSubtitles.
- Aviso legal visible en la landing (bloque breve + enlace a `/legal`): parte del catálogo tiene derechos de autor, descargar o compartir puede ser ilegal según el país, el uso es responsabilidad de quien la usa, y con BitTorrent también se comparte.
- Sin pósters de películas reales fuera de las capturas existentes.

## Assets
Copiar (no enlazar) al `public/` del repo web:
- `design/icon/app-icon.svg` (logo y favicon), `design/icon/preview.png`.
- `docs/screenshots/inicio.png`, `ficha.png`, `buscar.png` (inicio también como imagen Open Graph).
- Fuentes Archivo de `design/prototype/fonts/` (autoalojadas).

## Datos de la release (en build)
- `GET https://api.github.com/repos/GabJS10/yts-movie-player/releases/latest` en tiempo de build: `tag_name`, `published_at`, `html_url` y `assets[].browser_download_url`.
- Asociar asset por sufijo (no por nombre completo, la versión cambia):
  | Plataforma | Sufijo | Ejemplo v1.1.0 |
  |---|---|---|
  | Windows 10/11 | `_x64-setup.exe` | `YTS.Player_1.1.0_x64-setup.exe` |
  | Ubuntu/Debian/Mint/Pop!_OS | `_amd64.deb` | `YTS.Player_1.1.0_amd64.deb` |
  | Fedora/openSUSE | `.x86_64.rpm` | `YTS.Player-1.1.0-1.x86_64.rpm` |
  | Cualquier Linux | `_amd64.AppImage` | `YTS.Player_1.1.0_amd64.AppImage` |
- Si la API falla (rate limit, sin red), no romper el build: enlazar a `https://github.com/GabJS10/yts-movie-player/releases/latest`. Usar `GITHUB_TOKEN` del entorno si existe.
- Los comandos de instalación de `/descargar` se generan con el nombre real del asset.
- `/novedades`: `https://raw.githubusercontent.com/GabJS10/yts-movie-player/main/CHANGELOG.md` en build, renderizado como Markdown (fallback: copia local).
- `plan` añade en el repo de la app un workflow que dispara el build hook de Netlify al publicar una release, así la web se actualiza sola.

## Técnico
- Astro estático (sin SSR ni adaptador), Node 24. Tailwind opcional; los tokens como variables CSS.
- JS mínimo: detección de SO y animaciones de scroll.
- Accesible: foco visible (como la app), contraste AA, navegable con teclado, `alt` en capturas, `prefers-reduced-motion`.
- Responsive: de 375 px a 1920 px, sin scroll horizontal.
- SEO: `title`/`description` por página, Open Graph y Twitter card con `inicio.png`, `@astrojs/sitemap`, `robots.txt`, `lang="es"`.
- `netlify.toml`:
  ```toml
  [build]
    command = "npm run build"
    publish = "dist"
  [build.environment]
    NODE_VERSION = "24"
  [[headers]]
    for = "/_astro/*"
    [headers.values]
      Cache-Control = "public, max-age=31536000, immutable"
  ```

## Netlify y GitHub
- Sitio: `netlify sites:create --name yts-player` (si está ocupado, `yts-player-app`). CLI ya instalada y con sesión (team "GabJS10's team").
- Crear el repo de GitHub (`gh repo create GabJS10/yts-movie-player-web --public --source . --push`) **solo con el OK del usuario**.
- Deploy continuo desde GitHub: conectar requiere OAuth en el navegador → el usuario corre `! netlify init` o lo hace en la UI. Mientras, `netlify deploy` (borrador) para vista previa y `netlify deploy --prod` solo con el OK del usuario.
- Crear un build hook ("GitHub release") y pasarle la URL a `plan` (para el secreto `NETLIFY_BUILD_HOOK` del repo de la app).

## Forma de trabajo
- Commits en inglés estilo `feat(web): …`, **sin** la línea `Co-Authored-By: Claude …`.
- Al terminar cada hito, avisar a `plan`: `herdr agent prompt plan "web: <hito> listo, commit <hash>. <notas>"` (sin `--wait`) y comprobar con `herdr agent get plan`.

## Hitos
0. **Prototipo de la landing con `impeccable`** según la dirección visual → aviso a `plan`; el usuario lo revisa antes de seguir.
1. Scaffold Astro, layout base (header, footer, tokens, fuentes) y landing según el prototipo aprobado, con datos de release reales → aviso.
2. `/descargar`, `/docs/*`, `/novedades`, `/legal` → aviso.
3. Pulido: revisión con Playwright a 375 y 1440 px, Lighthouse (≥ 95 en rendimiento, accesibilidad y SEO), repo en GitHub, sitio en Netlify y build hook → aviso con la URL.
