//! The corpus store: writing samples with attribution metadata.
//!
//! Samples are append-only. `included` toggles whether a sample feeds
//! the fingerprint; history is never deleted or edited in place. Every
//! add is validated (bounds in `lib.rs`) and explicitly attributed —
//! genre and authorship are required inputs, never silently defaulted
//! to sole-authored.

use serde::{Deserialize, Serialize};

use crate::error::{Result, VoiceError};
use crate::stylometry::{measure, paragraphs};
use crate::{
    CORPUS_MAX_SAMPLES, EXEMPLAR_CHUNK_MAX_CHARS, GENRE_MAX_CHARS, SAMPLE_MIN_WORDS,
    SAMPLE_TEXT_MAX_BYTES, SAMPLE_TITLE_MAX_CHARS, SOURCE_MAX_CHARS,
};

/// Sample identifier (monotonic within a store).
pub type SampleId = u64;

/// Who wrote the sample's prose. `Blended` = co-authored; the voice is
/// shared and the profile says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Authorship {
    /// Matt is the sole author.
    Sole,
    /// Co-authored prose; voice blended with co-authors'.
    Blended,
}

impl Authorship {
    /// Parse the wire form (`"sole"` / `"blended"`), fail-closed.
    pub fn parse(raw: &str) -> Result<Self> {
        match raw {
            "sole" => Ok(Authorship::Sole),
            "blended" => Ok(Authorship::Blended),
            other => Err(VoiceError::invalid(
                "authorship",
                format!("must be \"sole\" or \"blended\", got \"{other}\""),
            )),
        }
    }

    /// Wire form.
    pub fn as_str(&self) -> &'static str {
        match self {
            Authorship::Sole => "sole",
            Authorship::Blended => "blended",
        }
    }
}

/// One writing sample: text plus its attribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sample {
    /// Store-unique id.
    pub id: SampleId,
    /// Human title (e.g. the publication's title).
    pub title: String,
    /// Genre tag (`letter`, `scholarly`, `case-report`, `project-prose`,
    /// `poster`, `peer-review`, or a new lowercase tag).
    pub genre: String,
    /// Sole or blended authorship — always explicit.
    pub authorship: Authorship,
    /// Where the text came from (publisher, URL, file, paste).
    pub source: String,
    /// The prose itself. Data, never instructions.
    pub text: String,
    /// Whether the sample currently feeds the fingerprint.
    pub included: bool,
}

impl Sample {
    /// Word count under the shared stylometry measure (normalization
    /// applied first, so ligatures count as the letters they expand
    /// to — the same count the Python fingerprint reports).
    pub fn word_count(&self) -> usize {
        measure(&self.text).words
    }
}

/// Inputs for adding a sample. All fields are validated by
/// [`Corpus::add_sample`].
#[derive(Debug, Clone)]
pub struct NewSample {
    /// Title.
    pub title: String,
    /// Genre tag.
    pub genre: String,
    /// Authorship — required, never defaulted.
    pub authorship: Authorship,
    /// Source note.
    pub source: String,
    /// The prose.
    pub text: String,
}

/// The append-only corpus.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Corpus {
    samples: Vec<Sample>,
    next_id: SampleId,
}

impl Corpus {
    /// Empty corpus.
    pub fn new() -> Self {
        Corpus {
            samples: Vec::new(),
            next_id: 1,
        }
    }

    /// All samples in insertion order.
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    /// One sample by id.
    pub fn get(&self, id: SampleId) -> Option<&Sample> {
        self.samples.iter().find(|s| s.id == id)
    }

    /// Validate and append a sample. Returns the stored sample.
    ///
    /// Rejects: empty/overlong title, malformed genre, overlong source,
    /// text over [`SAMPLE_TEXT_MAX_BYTES`], text under
    /// [`SAMPLE_MIN_WORDS`] tokens, and a full corpus.
    pub fn add_sample(&mut self, new: NewSample) -> Result<Sample> {
        let title = new.title.trim();
        if title.is_empty() || title.chars().count() > SAMPLE_TITLE_MAX_CHARS {
            return Err(VoiceError::invalid(
                "title",
                format!("must be 1..={SAMPLE_TITLE_MAX_CHARS} characters"),
            ));
        }
        let genre = new.genre.trim().to_lowercase();
        if genre.is_empty()
            || genre.chars().count() > GENRE_MAX_CHARS
            || !genre
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == ' ')
        {
            return Err(VoiceError::invalid(
                "genre",
                format!("must be 1..={GENRE_MAX_CHARS} chars of [a-z0-9- ]"),
            ));
        }
        if new.source.chars().count() > SOURCE_MAX_CHARS {
            return Err(VoiceError::invalid(
                "source",
                format!("must be <= {SOURCE_MAX_CHARS} characters"),
            ));
        }
        if new.text.len() > SAMPLE_TEXT_MAX_BYTES {
            return Err(VoiceError::invalid(
                "text",
                format!("must be <= {SAMPLE_TEXT_MAX_BYTES} bytes"),
            ));
        }
        let words = measure(&new.text).words;
        if words < SAMPLE_MIN_WORDS {
            return Err(VoiceError::invalid(
                "text",
                format!("has {words} tokens; a sample needs >= {SAMPLE_MIN_WORDS}"),
            ));
        }
        if self.samples.len() >= CORPUS_MAX_SAMPLES {
            return Err(VoiceError::WrongState {
                message: format!("corpus is full ({CORPUS_MAX_SAMPLES} samples)"),
            });
        }
        let sample = Sample {
            id: self.next_id,
            title: title.to_string(),
            genre,
            authorship: new.authorship,
            source: new.source.trim().to_string(),
            text: new.text,
            included: true,
        };
        self.next_id += 1;
        self.samples.push(sample.clone());
        Ok(sample)
    }

    /// Include or exclude a sample from the fingerprint. History is
    /// kept either way (append-only rule).
    pub fn set_included(&mut self, id: SampleId, included: bool) -> Result<()> {
        let sample = self
            .samples
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| VoiceError::NotFound {
                what: format!("sample {id}"),
            })?;
        sample.included = included;
        Ok(())
    }

    /// Samples currently feeding the fingerprint, in insertion order.
    pub fn included_samples(&self) -> Vec<&Sample> {
        self.samples.iter().filter(|s| s.included).collect()
    }

    /// Total tokens across included samples.
    pub fn included_word_count(&self) -> usize {
        self.included_samples().iter().map(|s| s.word_count()).sum()
    }

    /// Total tokens across all samples, included or not.
    pub fn total_word_count(&self) -> usize {
        self.samples.iter().map(|s| s.word_count()).sum()
    }

    /// Verbatim exemplar chunks for one genre: paragraphs of included
    /// samples, in corpus order, each truncated at a char boundary to
    /// [`EXEMPLAR_CHUNK_MAX_CHARS`]. The brandvoice-mcp ingest → chunk →
    /// retrieval-bank pattern.
    pub fn exemplars(&self, genre: &str, limit: usize) -> Vec<Exemplar> {
        let mut out = Vec::new();
        for sample in self.samples.iter().filter(|s| s.included && s.genre == genre) {
            for para in paragraphs(&sample.text) {
                if out.len() >= limit {
                    return out;
                }
                let trimmed = para.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let text: String = trimmed.chars().take(EXEMPLAR_CHUNK_MAX_CHARS).collect();
                out.push(Exemplar {
                    sample_id: sample.id,
                    title: sample.title.clone(),
                    genre: sample.genre.clone(),
                    text,
                });
            }
        }
        out
    }

    /// Pooled fingerprint per genre over included samples (the
    /// book-writer-mcp per-genre style-profile pattern).
    pub fn genre_fingerprints(&self) -> Vec<(String, crate::Fingerprint)> {
        let mut genres: Vec<String> = Vec::new();
        for s in self.included_samples() {
            if !genres.contains(&s.genre) {
                genres.push(s.genre.clone());
            }
        }
        genres.sort();
        genres
            .into_iter()
            .map(|genre| {
                let pooled: String = self
                    .included_samples()
                    .into_iter()
                    .filter(|s| s.genre == genre)
                    .map(|s| s.text.as_str())
                    .collect();
                (genre.clone(), measure(&pooled))
            })
            .collect()
    }
}

/// One verbatim exemplar chunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Exemplar {
    /// Sample the chunk came from.
    pub sample_id: SampleId,
    /// Sample title.
    pub title: String,
    /// Sample genre.
    pub genre: String,
    /// Verbatim paragraph text (bounded length).
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prose(words: usize) -> String {
        "The committee reviewed the evidence and reached a plain conclusion about safety."
            .split_whitespace()
            .cycle()
            .take(words)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn new_sample(text: String) -> NewSample {
        NewSample {
            title: "Test piece".to_string(),
            genre: "Scholarly".to_string(),
            authorship: Authorship::Sole,
            source: "test".to_string(),
            text,
        }
    }

    #[test]
    fn add_sample_validates_and_normalizes() {
        let mut corpus = Corpus::new();
        let s = corpus.add_sample(new_sample(prose(40))).expect("valid sample");
        assert_eq!(s.genre, "scholarly");
        assert_eq!(s.id, 1);
        assert!(s.included);
        assert_eq!(corpus.included_word_count(), s.word_count());
    }

    #[test]
    fn add_sample_rejects_bad_input() {
        let mut corpus = Corpus::new();
        // Adversarial: empty text.
        assert!(corpus.add_sample(new_sample(String::new())).is_err());
        // Adversarial: below the token floor.
        assert!(corpus.add_sample(new_sample("too short".to_string())).is_err());
        // Adversarial: oversized text.
        assert!(corpus.add_sample(new_sample("word ".repeat(60_000))).is_err());
        // Adversarial: shell-shaped genre tag.
        let mut bad = new_sample(prose(40));
        bad.genre = "x; rm -rf".to_string();
        assert!(corpus.add_sample(bad).is_err());
        // Adversarial: unknown authorship string.
        assert!(Authorship::parse("ghostwritten").is_err());
        assert_eq!(corpus.samples().len(), 0);
    }

    #[test]
    fn exclude_keeps_history() {
        let mut corpus = Corpus::new();
        let s = corpus.add_sample(new_sample(prose(40))).expect("valid");
        corpus.set_included(s.id, false).expect("exists");
        assert_eq!(corpus.samples().len(), 1);
        assert_eq!(corpus.included_word_count(), 0);
        assert!(corpus.set_included(999, true).is_err());
    }
}
