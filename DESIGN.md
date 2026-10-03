---
name: YTS Player
description: A lean-back desktop client for the YTS catalog where release facts and swarm health are first-class typography.
colors:
  ground: "#171717"
  surface: "#1d1d1d"
  raised-hi: "#2f2f2f"
  line: "#333333"
  line-hi: "#4a4a4a"
  text: "#ffffff"
  text-2: "#c9c9c9"
  muted: "#919191"
  faint: "#6b6b6b"
  green: "#6ac045"
  green-hi: "#75c74e"
  on-green: "#0b1606"
  green-wash: "rgba(106, 192, 69, 0.12)"
  warn: "#e3b341"
  danger: "#e5534b"
typography:
  display:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "clamp(3rem, 6.4vw, 6rem)"
    fontWeight: 850
    lineHeight: 0.9
    letterSpacing: "-0.01em"
    fontVariation: "'wdth' 72"
  display-detail:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "clamp(2.5rem, 5vw, 4.75rem)"
    fontWeight: 850
    lineHeight: 0.92
    fontVariation: "'wdth' 72"
  headline:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "clamp(2rem, 3.4vw, 3rem)"
    fontWeight: 850
    lineHeight: 1
    letterSpacing: "0.005em"
    fontVariation: "'wdth' 75"
  title:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 750
    lineHeight: 1.25
    letterSpacing: "0.005em"
  body:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.5
  body-lead:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "16.5px"
    fontWeight: 400
    lineHeight: 1.6
  label:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "11.5px"
    fontWeight: 600
    letterSpacing: "0.08em"
  tag:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 600
    lineHeight: 1.83
    letterSpacing: "0.02em"
    fontFeature: "'tnum' 1"
  numeral:
    fontFamily: "Archivo, Arimo, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 800
    fontFeature: "'tnum' 1"
    fontVariation: "'wdth' 85"
rounded:
  xs: "2px"
  sm: "3px"
  md: "4px"
  lg: "8px"
  full: "9999px"
spacing:
  gutter: "clamp(16px, 4vw, 56px)"
  track-gap: "8px"
  row-gap: "34px"
  section: "40px"
components:
  button-play:
    backgroundColor: "{colors.green}"
    textColor: "{colors.on-green}"
    rounded: "{rounded.md}"
    padding: "0 26px 0 22px"
    height: "48px"
  button-play-hover:
    backgroundColor: "{colors.green-hi}"
    textColor: "{colors.on-green}"
  button-ghost:
    backgroundColor: "rgba(110, 110, 110, 0.38)"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: "0 26px 0 22px"
    height: "48px"
  button-line:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: "0 26px 0 22px"
    height: "48px"
  button-sm:
    rounded: "{rounded.md}"
    padding: "0 14px"
    height: "36px"
  release-tag:
    backgroundColor: "rgba(23, 23, 23, 0.55)"
    textColor: "{colors.text-2}"
    typography: "{typography.tag}"
    rounded: "{rounded.sm}"
    height: "24px"
  poster-card:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.md}"
    width: "clamp(140px, 13.2vw, 210px)"
  continue-card:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.md}"
    width: "clamp(260px, 24vw, 380px)"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    padding: "0 12px"
    height: "38px"
  segmented-pressed:
    backgroundColor: "{colors.green}"
    textColor: "{colors.on-green}"
    height: "36px"
  nav-link:
    textColor: "{colors.text-2}"
    rounded: "{rounded.md}"
    padding: "8px 12px"
  nav-link-active:
    textColor: "{colors.text}"
  menu:
    backgroundColor: "rgba(29, 29, 29, 0.97)"
    rounded: "{rounded.lg}"
    width: "300px"
---

# Design System: YTS Player

## Overview

**Creative North Star: "The Release Sheet After Dark"**

YTS Player wears Netflix's lean-back grammar (full-bleed hero, horizontal poster rows, hover-expanding cards, a player that owns the whole window) but refuses to hide the torrent. Quality, codec, size, seeds and peers are set as first-class typography: a segmented release plate and a five-bar swarm signal travel with every title, from the hero to the poster hover to the download list. The viewer should know whether a film will stream before they press play.

The room is dim and the use is evening, so the ground is the near-black grey ladder of yts.gg, the artwork does the talking, and the chrome stays quiet. One colour signals: YTS green means play, live, healthy, selected and progress. Everything else is greys and white. Titles are Archivo pulled condensed and heavy in uppercase; the UI is the same family at normal width; every number is tabular so columns of seeds and sizes align.

The signature moment is the buffering screen: a live piece map whose cells fill green ahead of the playhead while peers, speed and MB count up, then dissolves into the first frame.

**Key Characteristics:**
- Dark yts.gg greys with YTS green as the single signal colour.
- Archivo variable: condensed heavy uppercase titles, normal-width UI, tabular numerals everywhere.
- Release facts as segmented hairline plates (`1080p | BluRay | x264 | 2,4 GB`).
- Swarm health as a five-bar signal whose state is form, never hue.
- Flat surfaces at rest; scrims over artwork; lift only on hover and for floating layers.
- Spanish UI copy ("Reproducir", "Más info", "Continuar viendo", "Abrir en VLC").

### Imagery

Artwork is sourced, never generated. Posters are YTS medium covers (mirrored from `img.yts.gg`). Hero, detail backdrop, "Continuar viendo" frames and the player's buffering frame use YTS **large screenshots** (`movie_details.json?with_images=true`, `large_screenshot_image1..3`), not `background_image`, because YTS pre-blurs `background_image`. When no screenshot exists, the cover itself is blurred as a fallback backdrop (`blur(28px) saturate(1.2)`, scaled 1.25, 70% opacity). Prototype rasters carry their source URL and fetch date in an embedded JPEG comment; the shipped app loads them from the API at runtime.

## Colors

A near-black grey ladder from yts.gg, white and two greys of text, and one green that carries every positive signal.

### Primary
- **YTS Green** (`green`): the only signal colour. Play buttons, the active nav underline, rating stars, filled signal bars, filled buffer pieces, progress and download bars, the selected timetable row's radio, pressed segments, switches, focus rings, text selection, links ("Ver todo", "Quitar filtros").
- **Lit Green** (`green-hi`): hover state of the play button only.
- **Green Ink** (`on-green`): text and icons sitting on a green fill; never plain black or white.
- **Green Wash** (`green-wash`): the selected timetable row, the focused input's 3px halo, the live dot's ring.

The brand-pinned deep green (#4B9924, PRODUCT.md) is reserved; the build has no surface for it yet.

### Tertiary (approved warning extension)
- **Codec Amber** (`warn`): warning *text* only. Timetable notes ("HEVC: puede necesitar VLC", "Pocos seeds: arranque lento", "Requiere pantalla 3D") and the codec cell inside an HEVC release plate.
- **Stall Red** (`danger`): the stalled download's status text ("Sin seeds conectados") and the hairline outline that replaces its progress fill.

### Neutral
- **Night Ground** (`ground`): the page, the solid top bar, scrim end-stops, the tab bar.
- **Panel Grey** (`surface`): card placeholders behind loading art, inputs and selects, the trailer dialog head, the player menu.
- **Lifted Grey** (`raised-hi`): meter and download-bar tracks, the scrub tooltip, the toast.
- **Hairline** (`line`): table rules, section dividers, the storage strip, menu borders.
- **Strong Hairline** (`line-hi`): release-plate borders and dividers, input and select strokes, outline buttons, hollow radio rings, scrollbar thumb.
- **White** (`text`): titles, primary values, bold figures inside plates and signals.
- **Silver** (`text-2`): secondary UI text, synopsis, inactive nav links, table cells.
- **Ash** (`muted`): ledes, column and field labels, cast, legends, units. Clears AA on `ground`.
- **Smoke** (`faint`): decorative only: dot separators, hollow signal-bar outlines, placeholders. Not for text a user must read.

### Named Rules
**The One Signal Rule.** Green is the only accent. If something is positive, live, selected or "go", it is green; if it is not, it is grey. No second brand hue.

**The Form-Not-Hue Rule.** Swarm signal bars are always green. Health is carried by form: solid bar = held, hollow bar (1px `faint` outline) = missing, all hollow plus a diagonal strike and a struck-through label = dead ("Sin seeds"). Never turn bars amber or red for weak or dead swarms.

**The Warning Ink Rule.** Amber and red are an approved extension, justified by the product promise that every failure has a next action. They appear as warning text (and the stalled bar's hairline) beside that next action, never as fills, buttons, badges, backgrounds or signal bars.

## Typography

**Display Font:** Archivo, variable width 62–125 and weight 300–900 (with Arimo, system-ui)
**Body Font:** Archivo at normal width
**Label/Mono Font:** ui-monospace / JetBrains Mono only for server URLs, paths and the API key field

**Character:** One family, two widths. Condensed heavy uppercase titles read like a release sheet's header; the same face at normal width keeps the UI calm and legible.

### Hierarchy
- **Display** (850, `clamp(3rem, 6.4vw, 6rem)`, 0.9, width 72%, uppercase, balanced wrap): hero title only.
- **Display Detail** (850, `clamp(2.5rem, 5vw, 4.75rem)`, 0.92, width 72%, uppercase): movie page title; the buffering title uses the same voice at `clamp(2rem, 4vw, 3.25rem)`.
- **Headline** (850, `clamp(2rem, 3.4vw, 3rem)`, 1, width 75%, uppercase): page titles (Buscar, Mi lista, Descargas, Ajustes).
- **Title** (750, 20px): row titles ("Tendencias en YTS"), section titles ("Elige versión"). Card titles 14px/750; continue-watching titles 17px/800 at width 85%; settings sections 22px/800.
- **Body** (400, 15px, 1.5): default UI. **Body Lead** (16.5–17px, 1.5–1.6, `text-2`): synopsis, capped at 54ch (hero, two-line clamp) and 68ch (detail).
- **Label** (600, 11.5px, 0.08em, uppercase, `muted`): timetable column heads, field labels, buffer stat names, download group headings ("En curso", "En la biblioteca"), menu section heads. Labels name the thing beneath them; they are not decorative kickers above titles.
- **Numeral** (800, 28px, width 85%, tabular): buffer stats; storage values at 16px/700; range outputs at 800.

### Named Rules
**The Two Widths Rule.** Condensed (72–85%) is for titles and big figures; UI text is always normal width. Never condense body copy or controls.

**The Tabular Rule.** Every number (seeds, peers, sizes, speeds, ratings, times, percentages) is set with tabular numerals so figures align in plates, tables and counters.

## Layout

Full-bleed and edge-to-edge, held by one fluid gutter (`spacing.gutter`) for nav, hero copy, row heads and page padding. A 64px fixed top bar fades from an 85% black gradient to solid `ground` with a hairline once scrolled.

- **Hero:** `min(88vh, 900px)`, min 560px; copy block bottom-left, max 620px wide; left and bottom scrims. Rows start 40px up into the hero's bottom edge so "Continuar viendo" overlaps it.
- **Rows:** horizontal snap-scrolling tracks, 8px gap, 22px/26px vertical padding so hover-lifted cards are not clipped, 34px between rows. Edge arrows appear on hover; Arrow keys move focus card to card.
- **Grids:** `auto-fill` poster grid, `minmax(clamp(140px, 13vw, 190px), 1fr)`, 28px row and 10px column gap.
- **Detail:** 54vh backdrop; poster column `clamp(180px, 18vw, 260px)` pulled up into it; the release timetable and "Similares" span full width beneath.
- **Settings:** 200px sticky section nav plus a content column capped at 760px; sections separated by 40px and a hairline.
- **Responsive:** at ≤900px the nav links and swarm pill give way to a fixed 64px bottom tab bar, detail and settings collapse to one column, the piece map drops from 40 to 24 columns. At ≤520px buttons drop to 44px and the hero synopsis clamps to three lines.

## Elevation & Depth

Flat at rest. Depth over artwork comes from gradient scrims (left and bottom, ending in `ground`); depth in the UI comes from the grey ladder and hairlines, not shadows. Shadows appear only when something lifts on intent (a hovered card) or floats above the page (detail poster, player menu, toast, trailer dialog). Translucent controls over imagery (ghost button) use an 8px backdrop blur.

### Shadow Vocabulary
- **Card lift** (`box-shadow: 0 14px 30px rgba(0,0,0,.6)`): poster and continue cards on hover/focus, paired with scale 1.07 / 1.04.
- **Poster float** (`box-shadow: 0 24px 48px rgba(0,0,0,.6)`): the detail page poster over its backdrop.
- **Menu** (`box-shadow: 0 18px 40px rgba(0,0,0,.6)`): player audio/subtitle menu.
- **Toast** (`box-shadow: 0 12px 32px rgba(0,0,0,.5)`).
- **Dialog** (`box-shadow: 0 30px 80px rgba(0,0,0,.7)`): trailer modal over an 82% black backdrop.

### Named Rules
**The Lift-On-Intent Rule.** Nothing casts a shadow at rest except things that genuinely float. A card earns its shadow by being hovered or focused.

## Shapes

Small, quiet corners: 4px (`rounded.md`) on buttons, cards, inputs and nav links; 8px (`rounded.lg`) on floating layers (menu, toast, dialog, detail poster, server list); 3px on release plates, MPA badge and key caps; 2px on card quality chips. Circles for icon buttons, round toggle buttons, the play overlay, radios and status dots. Hairline 1px borders do the structural work; dashed hairlines mean HEVC.

**The Hairline Plate Rule.** Release facts are a single segmented plate: 24px tall, 1px `line-hi` border, 1px dividers between cells, quality in bold white first. Selected or recommended releases switch the border and quality to green; HEVC releases switch every stroke to dashed and the codec cell to amber.

## Components

### Buttons
Confident and few; play is always the brightest object on screen.
- **Shape:** gently squared (4px), 48px tall, icon leading at 22px, label 16px/700.
- **Play ("Reproducir"):** green fill, green-ink label; hover to Lit Green. One per view.
- **Ghost ("Más info"):** translucent grey over imagery with backdrop blur; hover deepens to 55%.
- **Line:** transparent with an inset `line-hi` hairline; hover sharpens the hairline to `text-2`.
- **Small:** 36px, 14px label, 18px icon.
- **Round toggle (Mi lista, trailer):** 48px circle, 40% white hairline on 60% ground; pressed turns stroke and icon green.
- **Icon button:** 40px circle, `text-2` icon, 8% white hover wash.
- **Press:** scale 0.97 with the expo ease-out.

### Release Plate (signature)
See The Hairline Plate Rule. Reused verbatim in hero, card hover (as compact quality chips), timetable, buffer screen, player top bar and download rows.

### Swarm Signal (signature)
Five ascending bars (4px wide, 2px gap, 14px tall, heights 30/47/64/82/100%) followed by a tabular label ("100+ seeds", "8 seeds · pocos seeds", "Sin seeds"). Levels by seeds: 0, <5, <15, <40, <90, ≥90. See The Form-Not-Hue Rule. A tooltip spells it out ("Salud del enjambre: …, N peers").

### Release Timetable
The quality selector is a ruled table, not a card grid: one numeral size, right-aligned figures, 54px rows, hairline rules. Columns: radio, quality (16px/850), type, codec, size, seeds/peers, signal, note. Hover washes 3.5% white; the chosen row takes `green-wash` and a green radio. Notes are amber warning text with a line icon.

### Cards / Containers
- **Poster card:** 2:3, 4px corners, `surface` placeholder. On hover/focus it scales 1.07 and lifts; an info panel rises from a near-black gradient with title, rating, year, runtime, quality chips and the swarm signal. A small green heart badge marks "Mi lista".
- **Continue card:** 16:9 screenshot, bottom scrim, title and "Quedan … · m:ss de m:ss", 4px green progress bar flush to the bottom edge; a 54px outlined play disc fades in on hover.
- **Ruled lists:** downloads, storage, settings rows and buffer stats are open rows divided by hairlines, not boxed cards.

### Inputs / Fields
- **Style:** 38px, `surface` fill, 1px `line-hi` stroke, 4px corners; selects 36px with an inline chevron.
- **Hover / Focus:** stroke to `muted` on hover; green stroke plus a 3px `green-wash` halo on focus.
- **Search:** an open 72px field with only a 2px bottom rule, 28–40px bold text at width 85%; the rule turns green on focus.
- **Segmented control:** 36px, hairline-divided segments; pressed segment fills green with green-ink text.
- **Switch:** 44×24 track, `line-hi` off, green on.

### Navigation
- **Top bar:** "YTS" wordmark in 900 green at width 75% beside a tracked uppercase "PLAYER"; links 14px/500 in `text-2`, active white 700 with a 2px green underline inset 12px; right side carries the swarm pill (live green dot, speed, active count), search, downloads with a green count badge, and settings.
- **Tab bar (≤900px):** fixed bottom, 64px, `muted` labels, active in green.

### Buffering Screen (signature)
A 640px panel over the blurred, dimmed frame: condensed title, release plate and signal, a pulsing green phase dot ("Conectando al enjambre…"), a 40-column piece map (hollow = pending, green-outlined = wanted next, solid green = held), a legend, and three ruled stats (peers, speed, MB ready) in tabular numerals. When ready, the frame un-blurs over 1.1–1.6s while the panel fades and scales out.

### Player
Full-black stage, chrome hidden after idle. Scrub rail 4px (7px on hover) with played in green, buffered in 55% white and downloadable ranges as hollow white outlines; a green knob appears on hover. Subtitles 20–30px/600 white with a layered black shadow, lifting when chrome shows. The codec-error state offers "Abrir en VLC" as the primary (green) action and the x264 alternative beside it.

## Do's and Don'ts

### Do:
- **Do** keep green the only accent and use it for every positive state: play, live, held, selected, progress.
- **Do** show swarm health with the five-bar signal in green, changing only form: solid held, hollow missing, struck when dead.
- **Do** confine amber (`warn`) and red (`danger`) to warning text next to a recovery action (timetable notes, the HEVC codec cell, stalled download status and its bar outline).
- **Do** set every figure with tabular numerals and right-align numeric columns.
- **Do** carry release facts in the segmented hairline plate; dashed strokes and an amber codec cell for HEVC.
- **Do** set titles in Archivo condensed (72–85%) heavy uppercase and keep UI text at normal width.
- **Do** use YTS large screenshots for hero and backdrop imagery, with left and bottom scrims into `ground`.
- **Do** keep a visible 2px green focus outline (3px offset) on every interactive element and collapse transitions under reduced motion.

### Don't:
- **Don't** colour signal bars amber or red, or use any colour as the sole carrier of swarm state.
- **Don't** use amber or red as fills, buttons, badges or backgrounds.
- **Don't** use `background_image` from the YTS API as a hero; it arrives pre-blurred.
- **Don't** generate or stock-source artwork; every raster comes from YTS.
- **Don't** add shadows to resting surfaces; lift is for hover and floating layers.
- **Don't** condense body copy, controls or labels.
- **Don't** set readable text in `faint`; it is for separators, hollow strokes and placeholders.
- **Don't** bury quality, size or seeds behind a click; they travel with the title.

## Implementation Notes

For the Tauri 2 + React + Tailwind app (AGENTS.md):
- **Tokens to Tailwind.** Map the frontmatter colours, radii and spacing into the Tailwind theme, exposed as CSS custom properties (e.g. `--color-ground`, `--color-green`) so utilities and hand-written component CSS share one source. Typography roles become component classes or `@apply` groups because they need `font-stretch` / `wdth` and `tnum`.
- **Archivo self-hosted.** The app works offline: ship the variable woff2 files (latin + latin-ext, wdth 62–125, wght 300–900, OFL) from `design/prototype/fonts/` as local assets; no Google Fonts request. Fallback stack Arimo, system-ui.
- **Language.** UI copy is Spanish. API content (synopsis, titles, cast) arrives in English and is shown as delivered; genres are mapped to Spanish labels client-side and release types normalised to display labels ("BluRay", "WEB"). Layouts must tolerate both lengths.
- **"Similares"** is fed by `movie_suggestions.json`; the prototype's genre-based list is a stand-in.
- **Imagery.** Request `movie_details.json?with_images=true` and use `large_screenshot_image*` for hero, backdrop, continue-watching and the buffering frame; covers come from `medium_cover_image`.
