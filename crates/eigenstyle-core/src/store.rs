//! The on-disk store: corpus, fingerprint, settings (avoid list), and
//! the readable profile, as JSON/markdown files under one directory.
//!
//! - Location: `$EIGENSTYLE_HOME`, else `~/.local/share/eigenstyle/`.
//! - A fresh directory is seeded from the matt-voice skill's corpus,
//!   fingerprint, and profile (embedded at build time from `seed/`).
//! - Writes are atomic (temp file + rename in the same directory) so a
//!   crash never leaves a torn store.
//! - Recompute preserves user corrections (the overlay lives in the
//!   fingerprint document and is carried across explicitly).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::corpus::{Authorship, Corpus, NewSample};
use crate::error::{Result, VoiceError};
use crate::fingerprint::{Bands, FeatureDelta, FingerprintDoc};

const SEED_CORR: &str = include_str!("../seed/corr_letter.txt");
const SEED_README: &str = include_str!("../seed/donoharm_readme_prose.txt");
const SEED_JAS: &str = include_str!("../seed/jas_corpus.txt");
const SEED_CORAIL: &str = include_str!("../seed/PMC9201968.txt");
const SEED_ACDF: &str = include_str!("../seed/PMC8715042.txt");
const SEED_POSTER: &str = include_str!("../seed/donoharm_poster.txt");
const SEED_FINGERPRINT: &str = include_str!("../seed/fingerprint.json");
const SEED_PROFILE: &str = include_str!("../seed/profile.md");

/// The avoid list seeded from the profile's negative checklist
/// (HumanifyMe/Arcitext pattern: voice-negative markers the checker
/// surfaces in drafts).
pub const DEFAULT_AVOID_LIST: &[&str] = &[
    "furthermore",
    "moreover",
    "in conclusion",
    "it is important to note",
    "delve",
    "leverage",
    "landscape",
    "pivotal",
    "harnessing",
    "revolutionary",
    "game-changer",
    "10x",
];

struct SeedPiece {
    title: &'static str,
    genre: &'static str,
    authorship: Authorship,
    source: &'static str,
    text: &'static str,
}

const SEED_PIECES: &[SeedPiece] = &[
    SeedPiece {
        title: "Value-based Healthcare: Can Generative AI and LLMs be a Catalyst for Value-based Healthcare? (CORR letter, 2024)",
        genre: "letter",
        authorship: Authorship::Sole,
        source: "Publisher PDF from Matt's own copy; ORCID-listed, PMCID PMC11008637; body prose only",
        text: SEED_CORR,
    },
    SeedPiece {
        title: "First, Do No Harm — project prose (repo README, March 2025)",
        genre: "project-prose",
        authorship: Authorship::Blended,
        source: "Zenodo record 15062578 snapshot of qompassai/DoNoHarm; Porter 1st of 2 authors",
        text: SEED_README,
    },
    SeedPiece {
        title: "The Joint Awareness Score (Arthroplasty Today 2023) — first 60%",
        genre: "scholarly",
        authorship: Authorship::Blended,
        source: "Europe PMC full text, PMC10641080; Porter 1st of 6 authors",
        text: SEED_JAS,
    },
    SeedPiece {
        title: "Progressive proximal loosening of bilateral Corail stems: case report and review (Acta Orthopaedica 2022)",
        genre: "case-report",
        authorship: Authorship::Blended,
        source: "Europe PMC full text, PMC9201968; Porter 4th of 5 authors",
        text: SEED_CORAIL,
    },
    SeedPiece {
        title: "ACDF and posterior spinal fusion revision case report (Int J Surg Case Rep 2021)",
        genre: "case-report",
        authorship: Authorship::Blended,
        source: "Europe PMC full text, PMC8715042; Porter 2nd of 2 authors",
        text: SEED_ACDF,
    },
    SeedPiece {
        title: "First, Do No Harm — poster text",
        genre: "poster",
        authorship: Authorship::Blended,
        source: "DoNoHarm repo archive (preprint DOI 10.13140/rg.2.2.11768.35843/1); Porter 1st of 2 authors",
        text: SEED_POSTER,
    },
];

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    avoid_list: Vec<String>,
}

/// An opened store. Mutations are in-memory until a `save_*` call; the
/// MCP server and UIs save immediately after each mutation.
#[derive(Debug)]
pub struct Store {
    dir: PathBuf,
    /// The corpus (append-only).
    pub corpus: Corpus,
    /// The current fingerprint document (with corrections overlay).
    pub fingerprint: FingerprintDoc,
    /// Avoid-list phrases enforced by the checker.
    pub avoid_list: Vec<String>,
    /// The readable profile (markdown) served by `get_profile` and
    /// shown by the UIs.
    pub profile_markdown: String,
}

impl Store {
    /// Default store directory: `$EIGENSTYLE_HOME` or
    /// `~/.local/share/eigenstyle`.
    pub fn default_dir() -> PathBuf {
        if let Ok(home) = std::env::var("EIGENSTYLE_HOME")
            && !home.is_empty()
        {
            return PathBuf::from(home);
        }
        if let Some(home) = std::env::home_dir() {
            return home.join(".local").join("share").join("eigenstyle");
        }
        PathBuf::from(".eigenstyle")
    }

    /// Open the store at `dir`, seeding it from the skill's corpus,
    /// fingerprint, and profile when no corpus file exists yet.
    pub fn open(dir: &Path) -> Result<Store> {
        fs::create_dir_all(dir)?;
        let corpus_path = dir.join("corpus.json");
        if !corpus_path.exists() {
            let store = Store::seeded(dir.to_path_buf())?;
            store.save_all()?;
            return Ok(store);
        }
        let corpus: Corpus = serde_json::from_str(&fs::read_to_string(&corpus_path)?)?;
        let fingerprint: FingerprintDoc =
            serde_json::from_str(&fs::read_to_string(dir.join("fingerprint.json"))?)?;
        let settings_path = dir.join("settings.json");
        let avoid_list = if settings_path.exists() {
            let settings: Settings = serde_json::from_str(&fs::read_to_string(&settings_path)?)?;
            settings.avoid_list
        } else {
            DEFAULT_AVOID_LIST.iter().map(ToString::to_string).collect()
        };
        let profile_path = dir.join("profile.md");
        let profile_markdown = if profile_path.exists() {
            fs::read_to_string(&profile_path)?
        } else {
            SEED_PROFILE.to_string()
        };
        Ok(Store {
            dir: dir.to_path_buf(),
            corpus,
            fingerprint,
            avoid_list,
            profile_markdown,
        })
    }

    /// The in-memory seeded store (used by `open` on first run).
    fn seeded(dir: PathBuf) -> Result<Store> {
        let mut corpus = Corpus::new();
        for piece in SEED_PIECES {
            corpus.add_sample(NewSample {
                title: piece.title.to_string(),
                genre: piece.genre.to_string(),
                authorship: piece.authorship,
                source: piece.source.to_string(),
                text: piece.text.to_string(),
            })?;
        }
        let fingerprint: FingerprintDoc = serde_json::from_str(SEED_FINGERPRINT)?;
        Ok(Store {
            dir,
            corpus,
            fingerprint,
            avoid_list: DEFAULT_AVOID_LIST.iter().map(ToString::to_string).collect(),
            profile_markdown: SEED_PROFILE.to_string(),
        })
    }

    /// Directory the store persists to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Persist everything.
    pub fn save_all(&self) -> Result<()> {
        self.save_corpus()?;
        self.save_fingerprint()?;
        self.save_settings()?;
        self.save_profile()
    }

    /// Persist the corpus.
    pub fn save_corpus(&self) -> Result<()> {
        write_atomic(
            &self.dir.join("corpus.json"),
            &serde_json::to_string_pretty(&self.corpus)?,
        )
    }

    /// Persist the fingerprint document.
    pub fn save_fingerprint(&self) -> Result<()> {
        write_atomic(
            &self.dir.join("fingerprint.json"),
            &serde_json::to_string_pretty(&self.fingerprint)?,
        )
    }

    /// Persist settings (avoid list).
    pub fn save_settings(&self) -> Result<()> {
        let settings = Settings {
            avoid_list: self.avoid_list.clone(),
        };
        write_atomic(
            &self.dir.join("settings.json"),
            &serde_json::to_string_pretty(&settings)?,
        )
    }

    /// Persist the readable profile.
    pub fn save_profile(&self) -> Result<()> {
        write_atomic(&self.dir.join("profile.md"), &self.profile_markdown)
    }

    /// Add a phrase to the avoid list (idempotent, validated).
    pub fn add_avoid_phrase(&mut self, phrase: &str) -> Result<()> {
        let phrase = phrase.trim().to_lowercase();
        if phrase.is_empty() || phrase.chars().count() > 80 {
            return Err(VoiceError::invalid("phrase", "must be 1..=80 characters"));
        }
        if !self.avoid_list.contains(&phrase) {
            self.avoid_list.push(phrase);
        }
        Ok(())
    }

    /// Remove a phrase from the avoid list.
    pub fn remove_avoid_phrase(&mut self, phrase: &str) -> Result<()> {
        let before = self.avoid_list.len();
        self.avoid_list.retain(|p| p != &phrase.to_lowercase());
        if self.avoid_list.len() == before {
            return Err(VoiceError::NotFound {
                what: format!("avoid phrase {phrase}"),
            });
        }
        Ok(())
    }

    /// Recompute the fingerprint from the included corpus.
    ///
    /// With `dry_run = true` nothing is saved — the caller shows the
    /// proposed document and its deltas for correction first (Eden
    /// builder flow). Otherwise the new document replaces the current
    /// one, carrying the user's corrections across (entries for
    /// features that no longer exist are dropped), and is saved.
    ///
    /// Returns (applied document, feature deltas vs the previous
    /// computed reference, old bands, new bands).
    pub fn recompute(
        &mut self,
        dry_run: bool,
    ) -> Result<(FingerprintDoc, Vec<FeatureDelta>, Bands, Bands)> {
        let mut next = FingerprintDoc::build(&self.corpus)?;
        let deltas = next.diff(&self.fingerprint);
        let old_bands = self.fingerprint.bands;
        let new_bands = next.bands;
        next.corrections = self
            .fingerprint
            .corrections
            .iter()
            .filter(|(f, _)| next.reference.contains_key(*f))
            .map(|(f, c)| (f.clone(), *c))
            .collect();
        if !dry_run {
            self.fingerprint = next.clone();
            self.save_fingerprint()?;
        }
        Ok((next, deltas, old_bands, new_bands))
    }
}

/// Write `contents` to `path` via a same-directory temp file + rename.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("eigenstyle-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn fresh_store_is_seeded_from_the_skill_corpus() {
        let dir = temp_dir("seed");
        let store = Store::open(&dir).expect("opens");
        assert_eq!(store.corpus.samples().len(), 6);
        assert_eq!(store.corpus.included_word_count(), 7674);
        assert_eq!(store.fingerprint.features.len(), 45);
        assert!(!store.profile_markdown.is_empty());
        // Reopen: state round-trips.
        let again = Store::open(&dir).expect("reopens");
        assert_eq!(again.corpus, store.corpus);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recompute_dry_run_does_not_save() {
        let dir = temp_dir("dryrun");
        let mut store = Store::open(&dir).expect("opens");
        let before = store.fingerprint.clone();
        let (preview, _, _, _) = store.recompute(true).expect("preview builds");
        assert_eq!(store.fingerprint, before);
        let (applied, _, _, _) = store.recompute(false).expect("applies");
        assert_eq!(store.fingerprint, applied);
        assert_ne!(preview, before); // recomputed doc has title-keyed calibration
        let _ = fs::remove_dir_all(&dir);
    }
}
