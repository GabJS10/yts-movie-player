//! External player ("Abrir en VLC"): launching it and handing it the subtitles.
//!
//! Subtitles go as a local file (`--sub-file=<path>`, same flag in VLC and mpv). The
//! delay is applied to the cue times of a generated `.srt` instead of a player option:
//! VLC 3.0.20 has `--sub-delay` (in tenths of a second), but it belongs to its text
//! subtitle demuxer and does not apply to `.vtt` files, which VLC opens with a separate
//! `webvtt` demuxer (checked with `cvlc -vv`). Shifting the cues ourselves is exact to
//! the millisecond and behaves the same in every player.
//!
//! A subtitle problem never stops the player from opening: it opens without them and the
//! result says why.

use std::future::Future;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::subtitles::SubtitlesClient;
use crate::types::{ExternalSubtitle, SubtitleOption};
use crate::vtt;

/// Folder (inside the subtitles cache) for the `.srt` files handed to external players.
pub const EXTERNAL_DIR: &str = "external";
/// Delays beyond ±10 min are clamped (the front moves in 0.1 s steps).
pub const MAX_DELAY_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerKind {
    Vlc,
    Mpv,
    Unknown,
}

/// By the program name of the `externalPlayer` setting (`vlc`, `/usr/bin/cvlc`, `mpv`…).
pub fn player_kind(command: &str) -> PlayerKind {
    let name = Path::new(command.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    match name {
        "vlc" | "cvlc" | "qvlc" | "nvlc" | "svlc" => PlayerKind::Vlc,
        "mpv" => PlayerKind::Mpv,
        _ => PlayerKind::Unknown,
    }
}

/// Command-line arguments: the subtitle (if any) and then the stream URL.
pub fn player_args(kind: PlayerKind, url: &str, subtitle: Option<&Path>) -> Vec<String> {
    let mut args = Vec::new();
    if let (PlayerKind::Vlc | PlayerKind::Mpv, Some(sub)) = (kind, subtitle) {
        args.push(format!("--sub-file={}", sub.display()));
    }
    args.push(url.to_owned());
    args
}

/// What to do about subtitles, from the command arguments and the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtitleRequest {
    /// An OpenSubtitles file id chosen in the app.
    Id(String),
    /// A file the user loaded.
    File(PathBuf),
    /// Nothing chosen; automatic subtitles on and a key configured: search.
    Auto,
    /// Nothing chosen; automatic subtitles on but no key.
    NoKey,
    /// Subtitles turned off in the player ("Desactivados"), or nothing chosen and
    /// automatic subtitles off: no subtitles and no search.
    Nothing,
}

pub fn subtitle_request(
    subtitles_off: bool,
    subtitle_id: Option<String>,
    subtitle_path: Option<String>,
    auto_subtitles: bool,
    has_key: bool,
) -> SubtitleRequest {
    let non_empty = |v: Option<String>| v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    if subtitles_off {
        SubtitleRequest::Nothing
    } else if let Some(id) = non_empty(subtitle_id) {
        SubtitleRequest::Id(id)
    } else if let Some(path) = non_empty(subtitle_path) {
        SubtitleRequest::File(PathBuf::from(path))
    } else if !auto_subtitles {
        SubtitleRequest::Nothing
    } else if has_key {
        SubtitleRequest::Auto
    } else {
        SubtitleRequest::NoKey
    }
}

pub fn outcome_for(err: &AppError) -> ExternalSubtitle {
    match err {
        AppError::SubtitlesAuth(_) => ExternalSubtitle::NoKey,
        AppError::SubtitlesQuota(_) => ExternalSubtitle::Quota,
        AppError::NotFound(_) => ExternalSubtitle::NotFound,
        _ => ExternalSubtitle::Error,
    }
}

/// Writes `<out_dir>/<id>_<delay>.srt` from a cached `.vtt`, shifted by `delay_ms`.
/// Older files in `out_dir` are removed first (only the last one is ever needed).
pub fn write_external_srt(
    vtt_file: &Path,
    id: &str,
    delay_ms: i64,
    out_dir: &Path,
) -> AppResult<PathBuf> {
    let delay_ms = delay_ms.clamp(-MAX_DELAY_MS, MAX_DELAY_MS);
    let cues = vtt::parse_any(&std::fs::read(vtt_file)?);
    let shifted = vtt::shift(&cues, delay_ms);
    if shifted.is_empty() {
        return Err(AppError::InvalidInput(format!("subtitle {id} has no cues")));
    }
    std::fs::create_dir_all(out_dir)?;
    if let Ok(entries) = std::fs::read_dir(out_dir) {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let path = out_dir.join(format!("{id}_{delay_ms}.srt"));
    std::fs::write(&path, vtt::cues_to_srt(&shifted))?;
    Ok(path)
}

/// Resolves the subtitle for the external player. Never fails: problems come back as the
/// [`ExternalSubtitle`] outcome, with no file.
///
/// `search` is only awaited for [`SubtitleRequest::Auto`] (ranked results for the movie in
/// `subtitleLang`); the first result is used, from the disk cache when possible.
pub async fn prepare_subtitle<F>(
    client: &SubtitlesClient,
    kind: PlayerKind,
    request: SubtitleRequest,
    search: F,
    delay_ms: i64,
) -> (Option<PathBuf>, ExternalSubtitle)
where
    F: Future<Output = AppResult<Vec<SubtitleOption>>>,
{
    match (&request, kind) {
        (SubtitleRequest::Nothing, _) => return (None, ExternalSubtitle::None),
        // Don't search or spend quota on subtitles the player can't take.
        (_, PlayerKind::Unknown) => return (None, ExternalSubtitle::UnsupportedPlayer),
        (SubtitleRequest::NoKey, _) => return (None, ExternalSubtitle::NoKey),
        _ => {}
    }
    let id = match request {
        SubtitleRequest::Id(id) => client.load(&id).await.map(|_| id),
        SubtitleRequest::File(path) => client.import_file(&path).await.map(|(id, _)| id),
        SubtitleRequest::Auto => match search.await {
            Ok(options) => match options.into_iter().next() {
                Some(first) => client.load(&first.id).await.map(|_| first.id),
                None => Err(AppError::NotFound("no subtitles in that language".into())),
            },
            Err(e) => Err(e),
        },
        SubtitleRequest::Nothing | SubtitleRequest::NoKey => unreachable!("handled above"),
    };
    let result = id.and_then(|id| {
        let out_dir = client.subs_dir().join(EXTERNAL_DIR);
        write_external_srt(&client.cached_file(&id), &id, delay_ms, &out_dir)
    });
    match result {
        Ok(path) => (Some(path), ExternalSubtitle::Loaded),
        Err(e) => {
            tracing::warn!(error = %e, "external player opens without subtitles");
            (None, outcome_for(&e))
        }
    }
}

/// Finds the player binary: a path, or a name looked up in `PATH`.
pub fn resolve_player(command: &str) -> AppResult<PathBuf> {
    find_in_path(command.trim()).ok_or_else(|| {
        AppError::ExternalPlayerMissing(format!("{} not found in PATH", command.trim()))
    })
}

/// Launches the player detached.
pub fn launch(player: &Path, args: &[String]) -> AppResult<()> {
    let mut child = tokio::process::Command::new(player)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Reap the process when it exits.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if program.is_empty() {
        return None;
    }
    if candidate.components().count() > 1 {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|p| is_executable(p))
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:4321/stream/abc/0";

    #[test]
    fn detects_players() {
        assert_eq!(player_kind("vlc"), PlayerKind::Vlc);
        assert_eq!(player_kind("/usr/bin/cvlc"), PlayerKind::Vlc);
        assert_eq!(player_kind(" VLC.exe "), PlayerKind::Vlc);
        assert_eq!(player_kind("mpv"), PlayerKind::Mpv);
        assert_eq!(player_kind("/opt/mpv/mpv"), PlayerKind::Mpv);
        assert_eq!(player_kind("celluloid"), PlayerKind::Unknown);
        assert_eq!(player_kind("mpv-wrapper"), PlayerKind::Unknown);
        assert_eq!(player_kind(""), PlayerKind::Unknown);
    }

    #[test]
    fn builds_arguments() {
        let sub = Path::new("/home/u/.local/share/yts-player/subs/external/9003_-1500.srt");
        assert_eq!(
            player_args(PlayerKind::Vlc, URL, Some(sub)),
            [
                "--sub-file=/home/u/.local/share/yts-player/subs/external/9003_-1500.srt",
                URL
            ]
        );
        assert_eq!(
            player_args(PlayerKind::Mpv, URL, Some(sub)),
            [
                "--sub-file=/home/u/.local/share/yts-player/subs/external/9003_-1500.srt",
                URL
            ]
        );
        assert_eq!(player_args(PlayerKind::Vlc, URL, None), [URL]);
        // Unknown players get only the URL, whatever we have.
        assert_eq!(player_args(PlayerKind::Unknown, URL, Some(sub)), [URL]);
        // Spaces in the path stay inside one argument (no shell involved).
        let spaced = Path::new("/home/u/Mis subs/a.srt");
        assert_eq!(
            player_args(PlayerKind::Vlc, URL, Some(spaced))[0],
            "--sub-file=/home/u/Mis subs/a.srt"
        );
    }

    #[test]
    fn decides_the_request() {
        use SubtitleRequest::*;
        let s = |v: &str| Some(v.to_owned());
        assert_eq!(
            subtitle_request(false, s("9003"), s("/a.srt"), true, true),
            Id("9003".into())
        );
        assert_eq!(
            subtitle_request(false, s("9003"), None, false, false),
            Id("9003".into())
        );
        assert_eq!(
            subtitle_request(false, s(" "), s("/a.srt"), false, false),
            File("/a.srt".into())
        );
        assert_eq!(subtitle_request(false, None, None, true, true), Auto);
        assert_eq!(subtitle_request(false, None, s(""), true, false), NoKey);
        assert_eq!(subtitle_request(false, None, None, false, true), Nothing);
        // "Desactivados" wins over everything: no search, no file.
        assert_eq!(
            subtitle_request(true, s("9003"), s("/a.srt"), true, true),
            Nothing
        );
        assert_eq!(subtitle_request(true, None, None, true, true), Nothing);
    }

    #[test]
    fn maps_errors_to_outcomes() {
        assert_eq!(
            outcome_for(&AppError::SubtitlesAuth(String::new())),
            ExternalSubtitle::NoKey
        );
        assert_eq!(
            outcome_for(&AppError::SubtitlesQuota(String::new())),
            ExternalSubtitle::Quota
        );
        assert_eq!(
            outcome_for(&AppError::NotFound(String::new())),
            ExternalSubtitle::NotFound
        );
        assert_eq!(
            outcome_for(&AppError::Network(String::new())),
            ExternalSubtitle::Error
        );
        assert_eq!(
            outcome_for(&AppError::InvalidInput(String::new())),
            ExternalSubtitle::Error
        );
    }

    #[test]
    fn writes_the_shifted_srt_and_keeps_only_the_last() {
        let tmp = tempfile::tempdir().unwrap();
        let vtt = tmp.path().join("9003.vtt");
        std::fs::write(
            &vtt,
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<i>Hola</i> &amp; adiós\n\n",
        )
        .unwrap();
        let out = tmp.path().join(EXTERNAL_DIR);

        let first = write_external_srt(&vtt, "9003", 0, &out).unwrap();
        assert_eq!(first, out.join("9003_0.srt"));
        let later = write_external_srt(&vtt, "9003", 1_500, &out).unwrap();
        assert_eq!(
            std::fs::read_to_string(&later).unwrap(),
            "1\n00:00:02,500 --> 00:00:03,500\n<i>Hola</i> & adiós\n\n"
        );
        assert!(!first.exists());
        let earlier = write_external_srt(&vtt, "9003", -500, &out).unwrap();
        assert!(std::fs::read_to_string(&earlier)
            .unwrap()
            .contains("00:00:00,500 --> 00:00:01,500"));
        // Clamped to ±10 min.
        let far = write_external_srt(&vtt, "9003", 99 * 60 * 1000, &out).unwrap();
        assert!(std::fs::read_to_string(&far)
            .unwrap()
            .contains("00:10:01,000 --> 00:10:02,000"));
        // Shifted completely before 0: nothing to show.
        assert!(write_external_srt(&vtt, "9003", -5_000, &out).is_err());
    }

    #[test]
    fn missing_player_is_external_player_missing() {
        assert!(matches!(
            resolve_player("definitely-not-a-player-xyz"),
            Err(AppError::ExternalPlayerMissing(_))
        ));
        assert!(matches!(
            resolve_player(""),
            Err(AppError::ExternalPlayerMissing(_))
        ));
        assert!(resolve_player("sh").is_ok());
    }
}
