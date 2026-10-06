# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(Tauri 2 desktop app; the UI runs in a WebView — WebKitGTK on Linux, WebView2 (Chromium) on Windows since v1.1, macOS later. Paths and engine quirks come from the backend or are feature-detected, never assumed.)

## Stack

Decided in docs/PLAN.md: Tauri 2 + React + TypeScript + Vite, Tailwind, TanStack Query, TanStack Router, Zustand. Rust core for network, torrent, streaming and disk.

## Users

A single person on their own Linux or Windows desktop or laptop (1280–1920 px window, mouse and keyboard) who today downloads movies by hand from yts.gg and wants to browse the YTS catalog and press play instead.

## Product Purpose

Browse the YTS catalog (~77.5k movies) with a Netflix-style interface and stream any movie directly from its torrent while it downloads, with automatic Spanish subtitles, trailers, "Mi lista" and "Continuar viendo". Movies can also be saved to a local library. Success: from opening the app to the movie playing in under 30 seconds, with seeking working.

## Positioning

Not a streaming service: a desktop client over the public YTS catalog whose playback is a live torrent. Torrent truth (quality, size, seeds, peers, download speed, buffer) is part of the experience, not hidden plumbing.

## Operating Context

- Evening, lean-back use at a desk or on a laptop; the room is usually dim.
- Key flows: browse rows → open movie page → pick quality (seeing seeds/peers/size) → play; resume from "Continuar viendo"; search with filters; manage downloads; configure API key, cache limit and speeds in Ajustes.
- Playback waits on buffering (peers, speed, MB ready) before the first frame.

## Capabilities and Constraints

- Data from the YTS API v2: title, year, rating (IMDb), runtime, genres, synopsis, cast, cover images, background image, `yt_trailer_code`, torrents per quality (720p/1080p/2160p, x264/x265, size, seeds, peers).
- x265/HEVC may not play in WebKitGTK → "Abrir en VLC" fallback must be visible on codec errors; prefer x264.
- Subtitles from OpenSubtitles need the user's API key; ES by default, EN optional, manual .srt drop.
- Trailers via youtube-nocookie embed (may be blocked; fallback window).
- Keyboard navigable; player shortcuts: space, ←/→ ±10 s, F, M.
- UI copy is in Spanish or English (follows the system by default; selectable in Ajustes).

## Brand Commitments

- Dark, professional, Netflix-like aesthetic (user-pinned).
- Colors taken from yts.gg (user-pinned): green #6AC045 (hover #75C74E, deep #4B9924) on near-black greys #171717 / #1D1D1D / #2F2F2F, white text and #919191 secondary text.

## Evidence on Hand

No logo or brand assets of our own yet. Movie artwork and metadata come from the YTS API at runtime; prototypes use illustrative sample data that must be replaced by live API data.

## Product Principles

1. Play is always one decision away.
2. Show torrent health honestly (seeds, peers, speed, buffer) where it affects the choice.
3. The artwork leads; chrome stays quiet.
4. Never strand the user: every failure (no peers, codec, offline, API down) has a next action.

## Accessibility & Inclusion

Full keyboard navigation with visible focus; respect reduced motion; text contrast at WCAG AA on dark grounds.
