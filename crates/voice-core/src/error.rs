//! Typed errors for voice-core. Expected failures (bad input, missing
//! records, bound violations, I/O) are values, never panics.

use std::fmt;

/// Result alias used across voice-core.
pub type Result<T> = std::result::Result<T, VoiceError>;

/// Everything that can go wrong in voice-core.
#[derive(Debug)]
pub enum VoiceError {
    /// A field failed validation; `field` names the offending input.
    InvalidInput {
        /// Name of the rejected field.
        field: String,
        /// Why it was rejected (bounded, no echoed secrets).
        message: String,
    },
    /// A named record (sample, feature) does not exist.
    NotFound {
        /// What was looked up.
        what: String,
    },
    /// The request is well-formed but the current state forbids it
    /// (e.g. recompute with too few included pieces).
    WrongState {
        /// Explanation of the blocking state.
        message: String,
    },
    /// Filesystem failure while loading or saving the store.
    Io(std::io::Error),
    /// JSON encode/decode failure in store files or wire payloads.
    Json(serde_json::Error),
    /// URL fetch failure (network, status, bound) in the add flow.
    Fetch(String),
}

impl VoiceError {
    /// Convenience constructor for validation failures.
    pub fn invalid(field: &str, message: impl Into<String>) -> Self {
        VoiceError::InvalidInput {
            field: field.to_string(),
            message: message.into(),
        }
    }

    /// Stable machine-readable code for wire responses.
    pub fn code(&self) -> &'static str {
        match self {
            VoiceError::InvalidInput { .. } => "invalid_input",
            VoiceError::NotFound { .. } => "not_found",
            VoiceError::WrongState { .. } => "wrong_state",
            VoiceError::Io(_) => "io",
            VoiceError::Json(_) => "json",
            VoiceError::Fetch(_) => "fetch",
        }
    }
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VoiceError::InvalidInput { field, message } => {
                write!(f, "invalid input ({field}): {message}")
            }
            VoiceError::NotFound { what } => write!(f, "not found: {what}"),
            VoiceError::WrongState { message } => write!(f, "wrong state: {message}"),
            VoiceError::Io(e) => write!(f, "i/o error: {e}"),
            VoiceError::Json(e) => write!(f, "json error: {e}"),
            VoiceError::Fetch(m) => write!(f, "fetch failed: {m}"),
        }
    }
}

impl std::error::Error for VoiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VoiceError::Io(e) => Some(e),
            VoiceError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VoiceError {
    fn from(e: std::io::Error) -> Self {
        VoiceError::Io(e)
    }
}

impl From<serde_json::Error> for VoiceError {
    fn from(e: serde_json::Error) -> Self {
        VoiceError::Json(e)
    }
}
