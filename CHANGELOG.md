# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); [semantic](https://semver.org/) versioning.

## [Unreleased]

### Added
- The app is available in Spanish and English. By default it follows the system language (Spanish if the system prefers it, English otherwise); it can be changed in Settings › Language and the choice is remembered. The subtitle language is still a separate setting.

## [1.1.1] - 2026-10-05

### Changed
- "Open in VLC" is always available, with any version and not only with HEVC/4K ones: on the buffering screen ("Prefer VLC?") and in the player controls (button and the V key). It pauses the built-in video, leaves full screen and opens VLC with the active subtitle and its delay.

## [1.1.0] - 2026-10-05

Windows 10/11 support.

### Windows
- Installer for Windows 10/11 (`.exe`, per user, no administrator rights).
- Data, cache, downloads and logs in `%LOCALAPPDATA%\yts-player\`.
- VLC and mpv are found even when they're not on the `PATH` (registry and Program Files) and open without a console window.
- Moving downloads between drives, real free space, and files in use (VLC, the stream) are retried instead of failing.

### Changed
- With an HEVC version the app can't decode, the audio no longer keeps playing behind "Open in VLC".
- Long folder paths are shortened in the middle and always show the last folder.

## [1.0.0] - 2026-10-04

First public release. Linux: `.deb`, `.rpm` and AppImage.

### Catalog
- Netflix-style home with a rotating banner of recommendations based on your history ("Because you watched X", "Because it's in your list", your genres) and personalized rows.
- Search with genre, quality, rating and sort filters, kept when you come back.
- Movie page with synopsis, cast, stills, versions (quality, codec, seeds) and Similar movies.
- Configurable YTS API URL, with automatic failover between domains.

### Playback
- Streaming from the torrent with a ~8 MB buffer: the movie starts in seconds and you can skip to any point.
- Custom controls with shortcuts (space, ←/→, F, M, G/H) and a bar showing watched / buffered / downloaded.
- x265/HEVC versions: "Open in VLC" (or mpv) with the subtitles and their delay.
- "No peers" screen after 60 s, with alternatives.

### Subtitles
- Automatic Spanish subtitles from OpenSubtitles (free API key), ranked by how well they match the release.
- Adjustable delay, your own `.srt` through a dialog or drag and drop, a disk cache that doesn't use up the quota, and the OpenSubtitles page when the quota runs out.

### My list, Continue watching and downloads
- My list and Continue watching (resumes at the exact second; from 92 % on it counts as watched).
- Downloads to watch offline, with pause/resume that survives a restart and switching from streaming to download without downloading again.
- Selectable downloads and cache folders, with moving between drives.
- LRU cache limit, live speed limits and an option to keep seeding.

### Quality
- A screen or message with an action for every error; offline mode.
- Trailer inside the app (with fallbacks when YouTube doesn't allow it).
- Full keyboard navigation, visible focus, `prefers-reduced-motion` and AA contrast.
- Rotating log files, "About" and new version notices.
- Tests: Rust unit and integration tests (local torrent, no internet), Vitest on the frontend, E2E with WebdriverIO on the real app and Playwright + axe on the interface.

[Unreleased]: https://github.com/GabJS10/yts-movie-player/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/GabJS10/yts-movie-player/releases/tag/v1.1.1
[1.1.0]: https://github.com/GabJS10/yts-movie-player/releases/tag/v1.1.0
[1.0.0]: https://github.com/GabJS10/yts-movie-player/releases/tag/v1.0.0
