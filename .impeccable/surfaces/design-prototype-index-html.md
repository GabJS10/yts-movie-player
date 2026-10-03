---
version: 1
slug: "design-prototype-index-html"
primary_target: "design/prototype/index.html"
related_targets: []
---

# Surface: YTS Player app prototype (Home, Ficha, Reproductor, Buscar, Mi lista, Descargas, Ajustes)

Mode: Operate. Desktop window 1280–1920 px, mouse + keyboard, dim room, evening.
Task: find a movie, judge which torrent will actually play well, press play; resume; manage downloads; configure sources/subtitles/torrent.
Content: real YTS API v2 catalog sample (design/prototype/data.js, images in img/ and bg/). Download/progress/swarm figures are illustrative.
Constraints: user-pinned dark Netflix grammar and yts.gg palette; Spanish UI; keyboard navigable; prefer x264 and offer "Abrir en VLC".

## Direction contract

THESIS: Netflix's lean-back grammar (full-bleed hero, horizontal poster rows, hover-expanding cards) where the torrent is not hidden plumbing: release facts and swarm health are first-class typography. Refuses the generic clone that buries quality/seeds three clicks deep.

OWN-WORLD: yts.gg greys #171717 / #1D1D1D / #2F2F2F, white and #919191 text, YTS green #6AC045 as the single signal colour (play, live, healthy). Archivo, condensed heavy for titles, normal width for UI, tabular numerals for every number. Release tags are segmented hairline plates (1080p | x264 | 2.4 GB). Swarm health is a five-bar signal whose state is form: solid bars held, hollow bars missing, struck when dead. Raise from the emission-line rail (declined): state carried by line form, never hue alone. Raise from the timetable rack (declined): quality selector is a ruled timetable, one numeral size, columns aligned.

STORY: The viewer sees what is good tonight, sees at a glance whether it will stream, picks a quality knowingly, presses play and watches the swarm fill the buffer before the first frame.

FIRST VIEWPORT: Home at 1440×900: slim top bar (wordmark left, Inicio/Buscar/Mi lista/Descargas, search + ajustes right). Hero image fills ~78vh, left scrim; title in condensed 800 at ~5.5rem bottom-left third; meta line; two-line synopsis; release tag + swarm signal of the best torrent; green "Reproducir" and grey "Más info" side by side. "Continuar viendo" row overlaps the hero bottom edge.

FORM: Netflix canon translated through release-tag grammar; position 7 of 7 on the grounded list; seed key 61ce8585. Signature interaction: the player's buffering screen as a live piece map, sequential pieces filling ahead of the playhead with peers/speed/MB counting, dissolving into the first frame.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
