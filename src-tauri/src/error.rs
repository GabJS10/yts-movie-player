//! Application error type. Serialized to the frontend as `{ code, message }` (see `docs/IPC.md`).

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

/// Stable error codes shared with the frontend, which maps them to Spanish copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Network,
    ApiUnavailable,
    NotFound,
    InvalidInput,
    Torrent,
    NoPeers,
    SubtitlesAuth,
    SubtitlesQuota,
    ExternalPlayerMissing,
    Io,
    Db,
    Internal,
}

/// Error returned by every Tauri command. `message` is a technical detail (English, for logs).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("network error: {0}")]
    Network(String),
    #[error("all YTS API base URLs failed: {0}")]
    ApiUnavailable(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("torrent error: {0}")]
    Torrent(String),
    #[error("no peers: {0}")]
    NoPeers(String),
    #[error("OpenSubtitles auth error: {0}")]
    SubtitlesAuth(String),
    #[error("OpenSubtitles quota exceeded: {0}")]
    SubtitlesQuota(String),
    #[error("external player not found: {0}")]
    ExternalPlayerMissing(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Network(_) => ErrorCode::Network,
            Self::ApiUnavailable(_) => ErrorCode::ApiUnavailable,
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::InvalidInput(_) => ErrorCode::InvalidInput,
            Self::Torrent(_) => ErrorCode::Torrent,
            Self::NoPeers(_) => ErrorCode::NoPeers,
            Self::SubtitlesAuth(_) => ErrorCode::SubtitlesAuth,
            Self::SubtitlesQuota(_) => ErrorCode::SubtitlesQuota,
            Self::ExternalPlayerMissing(_) => ErrorCode::ExternalPlayerMissing,
            Self::Io(_) => ErrorCode::Io,
            Self::Db(_) => ErrorCode::Db,
            Self::Internal(_) => ErrorCode::Internal,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", &self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_as_code_and_message() {
        let err = AppError::NotFound("movie 42".into());
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            json!({ "code": "not_found", "message": "not found: movie 42" })
        );
    }

    #[test]
    fn io_errors_convert_and_use_io_code() {
        let err: AppError = std::io::Error::other("disk full").into();
        assert_eq!(
            serde_json::to_value(&err).unwrap(),
            json!({ "code": "io", "message": "io error: disk full" })
        );
    }

    #[test]
    fn every_variant_maps_to_its_snake_case_code() {
        let cases = [
            (AppError::Network(String::new()), "network"),
            (AppError::ApiUnavailable(String::new()), "api_unavailable"),
            (AppError::NotFound(String::new()), "not_found"),
            (AppError::InvalidInput(String::new()), "invalid_input"),
            (AppError::Torrent(String::new()), "torrent"),
            (AppError::NoPeers(String::new()), "no_peers"),
            (AppError::SubtitlesAuth(String::new()), "subtitles_auth"),
            (AppError::SubtitlesQuota(String::new()), "subtitles_quota"),
            (
                AppError::ExternalPlayerMissing(String::new()),
                "external_player_missing",
            ),
            (AppError::Io(std::io::Error::other("")), "io"),
            (AppError::Db(String::new()), "db"),
            (AppError::Internal(String::new()), "internal"),
        ];
        for (err, code) in cases {
            let value = serde_json::to_value(&err).unwrap();
            assert_eq!(value["code"], code);
            assert_eq!(value.as_object().unwrap().len(), 2);
        }
    }
}
