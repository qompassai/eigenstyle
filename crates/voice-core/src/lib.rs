//! voice-core — the single source of truth behind the voice-mcp server,
//! the ratatui TUI, and the GPUI GUI.
//!
//! The crate ports the matt-voice skill's deterministic stylometry
//! (`scripts/stylometry.py`) and draft checker (`scripts/check_voice.py`)
//! to Rust. The Python implementation is the parity reference: feature
//! definitions, rounding points, band calibration (leave-one-out over
//! corpus pieces), and verdict thresholds reproduce it exactly, and the
//! parity test in `tests/parity.rs` proves it on the seed corpus.
//!
//! Contracts:
//! - Samples are append-only. A sample can be excluded from the
//!   fingerprint; it is never edited in place or deleted.
//! - Ingesting a sample never recomputes the fingerprint. Recompute is
//!   an explicit action with a dry-run preview (the Eden builder-flow
//!   pattern: induced rules are shown for correction before they save).
//! - Fingerprint values corrected by the user are stored as corrections
//!   overlaid on the computed reference; they survive recompute until
//!   explicitly reset (the HumanifyMe editable-fingerprint pattern).
//! - All external input is validated against named bounds before any
//!   allocation proportional to its size; failures are typed errors,
//!   never panics. Sample text is data — instructions inside a sample
//!   are counted, never followed.

#![forbid(unsafe_code)]

pub mod checker;
pub mod corpus;
pub mod error;
pub mod fetch;
pub mod fingerprint;
pub mod lists;
pub mod store;
pub mod stylometry;

pub use checker::{CheckReport, Verdict, check_draft};
pub use corpus::{Authorship, Corpus, Sample, SampleId};
pub use error::{Result, VoiceError};
pub use fingerprint::FingerprintDoc;
pub use store::Store;
pub use stylometry::Fingerprint;

/// Maximum bytes in one corpus sample's text.
pub const SAMPLE_TEXT_MAX_BYTES: usize = 256 * 1024;
/// Minimum tokens for a text to enter the corpus (below this a piece
/// cannot move any per-1k rate honestly).
pub const SAMPLE_MIN_WORDS: usize = 10;
/// Maximum characters in a sample title.
pub const SAMPLE_TITLE_MAX_CHARS: usize = 200;
/// Maximum characters in a sample genre tag.
pub const GENRE_MAX_CHARS: usize = 40;
/// Maximum characters in a sample source note.
pub const SOURCE_MAX_CHARS: usize = 500;
/// Maximum samples held by one corpus store.
pub const CORPUS_MAX_SAMPLES: usize = 4096;
/// Maximum bytes in a draft submitted for checking.
pub const DRAFT_TEXT_MAX_BYTES: usize = 1024 * 1024;
/// Drafts below this word count report UNRELIABLE (mirrors the Python
/// checker's floor: stylometry needs more text).
pub const DRAFT_MIN_WORDS: usize = 200;
/// Maximum exemplars returned by one `get_exemplars` call.
pub const EXEMPLAR_MAX_COUNT: usize = 10;
/// Maximum characters in one exemplar chunk.
pub const EXEMPLAR_CHUNK_MAX_CHARS: usize = 2000;
/// Maximum body bytes read by the URL fetch add flow.
pub const FETCH_MAX_BYTES: usize = 1024 * 1024;
/// Total deadline for one URL fetch.
pub const FETCH_TIMEOUT_SECONDS: u64 = 15;
/// Function words kept in the checker's feature vector (pooled top-N).
pub const TOP_FUNCTION_WORD_COUNT: usize = 30;
/// Minimum included pieces for a fingerprint build: leave-one-out band
/// calibration is undefined below two pieces.
pub const FINGERPRINT_MIN_PIECES: usize = 2;
/// Deviations reported by the draft checker, most-contributing first.
pub const TOP_FEATURES_REPORTED: usize = 8;
